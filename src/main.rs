// Hide the console window in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod ui;

use gpui::{px, size, App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions};
use ifm_replay::player::Player;
use ui::player_view::PlayerView;

fn main() {
    Application::new().run(|cx: &mut App| {
        let player = match Player::new() {
            Ok(player) => player,
            Err(e) => {
                eprintln!("cannot open a UDP socket: {e}");
                cx.quit();
                return;
            }
        };
        // A file dropped on the executable, or `ifm-replay take.anim`, opens at launch.
        let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
        let bounds = Bounds::centered(None, size(px(560.), px(340.)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("iFM Replay".into()),
                    ..Default::default()
                }),
                window_min_size: Some(size(px(420.), px(280.))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| PlayerView::new(player, file, window, cx)),
        );
        if let Err(e) = opened {
            eprintln!("cannot open the window: {e}");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.activate(true);
    });
}
