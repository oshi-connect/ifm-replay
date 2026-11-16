use crate::ui::theme;
use gpui::prelude::*;
use gpui::{
    canvas, div, px, Bounds, ClickEvent, Context, ExternalPaths, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, PathPromptOptions, Pixels,
    SharedString, Window,
};
use ifm_replay::ifm::{self, parse_target, Mapped};
use ifm_replay::player::{Player, Status};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

struct Loaded {
    file: SharedString,
    duration: f64,
    channels: usize,
    eyes: bool,
    head: bool,
    unmatched: Vec<String>,
}

pub struct PlayerView {
    player: Player,
    status: Status,
    loaded: Option<Loaded>,
    error: Option<SharedString>,
    address: String,
    root_focus: FocusHandle,
    address_focus: FocusHandle,
    seek_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl PlayerView {
    pub fn new(
        player: Player,
        file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let address = format!("127.0.0.1:{}", ifm::DEFAULT_PORT);
        player.set_target(parse_target(&address));
        let root_focus = cx.focus_handle();
        window.focus(&root_focus);

        // Mirror the sender's clock into the view; it only redraws when something changed.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(33)).await;
            let alive = this.update(cx, |view, cx| {
                let status = view.player.status();
                if status != view.status {
                    view.status = status;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();

        let view = Self {
            status: player.status(),
            player,
            loaded: None,
            error: None,
            address,
            root_focus,
            address_focus: cx.focus_handle(),
            seek_bounds: Rc::new(Cell::new(None)),
        };
        if let Some(file) = file {
            cx.defer_in(window, move |view, _, cx| view.load(file, cx));
        }
        view
    }

    fn open(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await {
                if let Some(path) = paths.into_iter().next() {
                    this.update(cx, |view, cx| view.load(path, cx)).ok();
                }
            }
        })
        .detach();
    }

    fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn({
            let path = path.clone();
            async move {
                let text = std::fs::read_to_string(&path)?;
                ifm_replay::anim::parse(&text).map(Mapped::new)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |view, cx| {
                match result {
                    Ok(clip) => {
                        view.loaded = Some(Loaded {
                            file: path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default()
                                .into(),
                            duration: clip.duration,
                            channels: clip.channels.len(),
                            eyes: clip.left_eye.is_some() || clip.right_eye.is_some(),
                            head: clip.head.is_some(),
                            unmatched: clip.unmatched.clone(),
                        });
                        view.error = None;
                        view.player.load(clip);
                    }
                    Err(e) => view.error = Some(format!("{}: {e}", path.display()).into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn seek_to(&mut self, x: Pixels) {
        let (Some(bounds), Some(loaded)) = (self.seek_bounds.get(), &self.loaded) else {
            return;
        };
        let width = f64::from(bounds.size.width).max(1.);
        let fraction = (f64::from(x - bounds.origin.x) / width).clamp(0., 1.);
        self.player.seek(fraction * loaded.duration);
    }

    fn root_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "space" {
            self.player.toggle();
            cx.notify();
        }
    }

    fn address_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        match keystroke.key.as_str() {
            "backspace" => {
                self.address.pop();
            }
            "enter" | "escape" => window.focus(&self.root_focus),
            "v" if keystroke.modifiers.secondary() => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.address.push_str(text.trim());
                }
            }
            _ => match &keystroke.key_char {
                Some(c) if c.chars().all(|ch| ch.is_ascii_graphic()) => self.address.push_str(c),
                _ => return,
            },
        }
        self.player.set_target(parse_target(&self.address));
        cx.stop_propagation();
        cx.notify();
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .child(button("open", "Open .anim…", true).on_click(cx.listener(Self::open)))
    }

    fn render_clip(&self, loaded: &Loaded, cx: &mut Context<Self>) -> impl IntoElement {
        let fraction = if loaded.duration > 0. {
            (self.status.position / loaded.duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        let bounds = self.seek_bounds.clone();
        let summary = [
            Some(format!("{} of 52 channels", loaded.channels)),
            loaded.eyes.then(|| "eye bones".to_string()),
            loaded.head.then(|| "head".to_string()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(loaded.file.clone()),
                    )
                    .child(button("open", "Open…", false).on_click(cx.listener(Self::open))),
            )
            .child(
                div()
                    .id("seek")
                    .relative()
                    .h(px(18.))
                    .w_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(
                        canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                            .absolute()
                            .size_full(),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(6.))
                            .rounded_full()
                            .bg(theme::surface())
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(theme::accent())
                                    .w(gpui::relative(fraction)),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, e: &MouseDownEvent, _, cx| {
                            this.seek_to(e.position.x);
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                        if e.pressed_button == Some(MouseButton::Left) {
                            this.seek_to(e.position.x);
                            cx.notify();
                        }
                    })),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_sm()
                    .text_color(theme::muted())
                    .child(clock(self.status.position))
                    .child(clock(loaded.duration)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        button("play", if self.status.playing { "Pause" } else { "Play" }, true)
                            .w(px(88.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.player.toggle();
                                cx.notify();
                            })),
                    )
                    .child(
                        toggle("loop", "Loop", self.status.looping).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.player.set_looping(!this.status.looping);
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(div().text_sm().text_color(theme::muted()).child(summary))
            .when(!loaded.unmatched.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme::muted())
                        .child(format!("Not streamed: {}", loaded.unmatched.join(", "))),
                )
            })
    }

    fn render_target(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.address_focus.is_focused(window);
        let valid = parse_target(&self.address).is_some();
        div()
            .flex()
            .items_center()
            .gap_3()
            .text_sm()
            .child(div().text_color(theme::muted()).child("Send to"))
            .child(
                div()
                    .id("address")
                    .track_focus(&self.address_focus)
                    .flex()
                    .items_center()
                    .min_w(px(180.))
                    .h(px(30.))
                    .px_2()
                    .rounded(px(6.))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(if !valid {
                        theme::danger()
                    } else if focused {
                        theme::accent()
                    } else {
                        theme::border()
                    })
                    .cursor_text()
                    .on_click(cx.listener(|this, _, window, cx| {
                        window.focus(&this.address_focus);
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(Self::address_key))
                    .child(self.address.clone())
                    .when(focused, |this| {
                        this.child(div().w(px(1.)).h(px(16.)).bg(theme::text()))
                    }),
            )
            .when_some(self.status.error.clone(), |this, e| {
                this.child(div().text_color(theme::danger()).truncate().child(e))
            })
    }
}

impl Render for PlayerView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("root")
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::root_key))
            .on_click(cx.listener(|this, _, window, _| window.focus(&this.root_focus)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                if let Some(path) = paths.paths().first() {
                    this.load(path.clone(), cx);
                }
            }))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(theme::surface()))
            .size_full()
            .flex()
            .flex_col()
            .gap_5()
            .p_6()
            .bg(theme::bg())
            .text_color(theme::text())
            .child(match &self.loaded {
                Some(loaded) => self.render_clip(loaded, cx).into_any_element(),
                None => self.render_empty(cx).into_any_element(),
            })
            .when_some(self.error.clone(), |this, e| {
                this.child(div().text_sm().text_color(theme::danger()).child(e))
            })
            .when(self.loaded.is_some(), |this| this.child(div().flex_1()))
            .child(self.render_target(window, cx))
    }
}

fn clock(seconds: f64) -> String {
    let tenths = (seconds.max(0.) * 10.).round() as u64;
    format!("{:02}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

fn button(id: &'static str, label: &'static str, primary: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(32.))
        .px_4()
        .rounded(px(6.))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(primary, |this| this.bg(theme::accent()).text_color(gpui::white()))
        .when(!primary, |this| this.border_1().border_color(theme::border()))
        .hover(|style| style.opacity(0.85))
        .child(label)
}

fn toggle(id: &'static str, label: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .text_sm()
        .cursor_pointer()
        .child(
            div()
                .size(px(16.))
                .rounded(px(4.))
                .border_1()
                .border_color(if on { theme::accent() } else { theme::border() })
                .when(on, |this| this.bg(theme::accent())),
        )
        .child(label)
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_shows_minutes_seconds_and_tenths() {
        assert_eq!(super::clock(83.44), "01:23.4");
        assert_eq!(super::clock(0.), "00:00.0");
    }
}
