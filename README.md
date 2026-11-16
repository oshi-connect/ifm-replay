# iFM Replay

iFM Replay plays a recorded face animation into Warudo as a live iFacialMocap stream.

## Why

iFacialMocap can record a take as FBX. When Warudo plays that recording, the curves bind to the
mesh blendshapes by their source names. A model with ARKit set up on its VRM expressions, but
with differently named source blendshapes, does not move.

The live iFacialMocap stream goes through the VRM expression layer instead. This app reads the
recording and sends it as that stream, so the recording drives the face the same way live
tracking does.

## Use

1. Import the iFacialMocap FBX into Unity and save the clip as a text `.anim` file.
2. Start iFM Replay. Click **Open .anim…**, drop the file on the window, or drop it on the
   executable.
3. In Warudo, add an iFacialMocap receiver. Keep the default port, 49983.
4. Set **Send to** to the Warudo PC. Use `127.0.0.1` when both apps run on the same PC.
5. Click **Play** or press Space. Click or drag the bar to seek. **Loop** repeats the clip.

Names that the app cannot map to an iFacialMocap channel are listed under the controls.

## What it sends

- One UDP datagram about every 16.7 ms while a clip is open. While paused, it sends the current
  frame again. Warudo sets tracking to lost after 0.5 s with no datagram.
- The iFacialMocap text layout: `jawOpen-45|…|=head#x,y,z,0,0,0|rightEye#x,y,z|leftEye#x,y,z|`.
  Warudo reads only `name-value`. It ignores the `name&value` form.
- Weights from 0 to 100. Angles in degrees.
- All 52 channels in each datagram. A channel that the clip does not animate is sent as 0.
- The socket uses a random local port. Thus the app can run on the same PC as Warudo, which
  holds port 49983.

## How it reads the clip

- **Blendshapes:** the `blendShape.*` or `BlendShape.*` curves in `m_FloatCurves`, or in `m_EditorCurves` when
  `m_FloatCurves` has none. ARKit names (`eyeBlinkLeft`), iFacialMocap names (`eyeBlink_L`),
  and DCC prefixes (`blendShape1.eyeBlinkLeft`) map to the same channel. Case and `_` do not
  matter.
- **Curves:** Unity keeps only keyframes. The app interpolates with Unity's cubic Hermite
  tangents and holds the value on an infinite (stepped) tangent. It samples by wall-clock
  time, so a late frame does not slow the clip.
- **Eyes and head:** rotation curves (`m_RotationCurves`, `m_EulerCurves`, or the editor
  curves) on a bone whose name looks like a left eye, a right eye, or a head. The iFacialMocap
  FBX keeps the head rotation on its `face` root. The app sends Unity's Euler angles (ZXY order)
  of the local rotation. Warudo builds the rotation with `Quaternion.Euler`, so no axis changes.

## Build

```sh
cargo run --release -- path/to/take.anim
cargo test
```

Windows: build the x64 target with the MSVC toolchain. On an ARM64 Windows host, use the x64
toolchain (`cargo +stable-x86_64-pc-windows-msvc build --release`), because a dependency
needs clang for `aarch64-pc-windows-msvc`.

macOS compiles GPUI's Metal shaders at launch (`runtime_shaders`), so it does not need Xcode's
Metal toolchain.

`scripts/make_demo_anim.py out.anim` writes an 8 s test clip: a blink, jaw open, eyes right then
left (blendshapes and eye bones), and a smile.

## Checked

Warudo 0.15.1 on Windows 11, with an ARKit VRM and a 20.8 s iFacialMocap take (52 channels, eye
bones, head on `face`):

- The mouth, the blinks, the gaze, and the head follow the take.
- Gaze sign: at a frame with `eyeLookOut_L` 44 the eyes turn to the avatar's left. At a frame
  with `eyeLookIn_L` 28 they turn to the avatar's right.
- A paused frame stays on the avatar.

## Open items

- **Humanoid clips:** a clip imported as Humanoid stores the eyes as muscle curves
  (`Left Eye In-Out`). The app does not read those yet. Import the FBX as Generic.
- **Weighted tangents:** keys with `weightedMode` other than 0 are read as plain Hermite.
- **Direct FBX:** reading the iFacialMocap FBX without Unity is not done.
