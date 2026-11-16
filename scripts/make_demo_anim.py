"""Writes a demo Unity .anim clip in the layout an iFacialMocap FBX import produces.

Usage: python3 scripts/make_demo_anim.py out.anim
"""
import math
import sys

DURATION = 8.0


def float_curve(attribute, keys):
    lines = ["  - curve:", "      serializedVersion: 2", "      m_Curve:"]
    for t, v in keys:
        lines += [
            "      - serializedVersion: 3",
            f"        time: {t}",
            f"        value: {v}",
            "        inSlope: 0",
            "        outSlope: 0",
            "        tangentMode: 136",
            "        weightedMode: 0",
            "        inWeight: 0.33333334",
            "        outWeight: 0.33333334",
        ]
    lines += [
        "      m_PreInfinity: 2",
        "      m_PostInfinity: 2",
        "      m_RotationOrder: 4",
        f"    attribute: blendShape.{attribute}",
        "    path: Face",
        "    classID: 137",
        "    script: {fileID: 0}",
        "    flags: 0",
    ]
    return lines


def quat_y(degrees):
    half = math.radians(degrees) / 2
    return (0.0, math.sin(half), 0.0, math.cos(half))


def rotation_curve(path, keys):
    lines = ["  - curve:", "      serializedVersion: 2", "      m_Curve:"]
    for t, (x, y, z, w) in keys:
        lines += [
            "      - serializedVersion: 3",
            f"        time: {t}",
            f"        value: {{x: {x:.7f}, y: {y:.7f}, z: {z:.7f}, w: {w:.7f}}}",
            "        inSlope: {x: 0, y: 0, z: 0, w: 0}",
            "        outSlope: {x: 0, y: 0, z: 0, w: 0}",
            "        tangentMode: 0",
            "        weightedMode: 0",
            "        inWeight: {x: 0.33333334, y: 0.33333334, z: 0.33333334, w: 0.33333334}",
            "        outWeight: {x: 0.33333334, y: 0.33333334, z: 0.33333334, w: 0.33333334}",
        ]
    lines += [
        "      m_PreInfinity: 2",
        "      m_PostInfinity: 2",
        "      m_RotationOrder: 4",
        f"    path: {path}",
    ]
    return lines


def blink(times):
    keys = [(0, 0)]
    for t in times:
        keys += [(t - 0.08, 0), (t, 100), (t + 0.1, 0)]
    return keys + [(DURATION, 0)]


def main(out):
    # Unity avatars face +Z, so a positive Y rotation turns the eyes to the avatar's right.
    # The eyes look right at 3 s and left at 5 s (the face's own sides, as ARKit names them).
    gaze = [(0, 0), (2.6, 0), (3.0, 20), (3.8, 20), (4.6, -20), (5.4, -20), (6.0, 0), (DURATION, 0)]
    look_out_r = [(t, max(d, 0) * 5) for t, d in gaze]
    look_in_l = look_out_r
    look_out_l = [(t, max(-d, 0) * 5) for t, d in gaze]
    look_in_r = look_out_l
    blendshapes = {
        "eyeBlinkLeft": blink([1.0, 6.5]),
        "eyeBlinkRight": blink([1.0, 6.5]),
        "jawOpen": [(0, 0), (1.5, 0), (2.0, 80), (2.4, 0), (DURATION, 0)],
        "mouthSmileLeft": [(0, 0), (6.6, 0), (7.2, 90), (DURATION, 90)],
        "mouthSmileRight": [(0, 0), (6.6, 0), (7.2, 90), (DURATION, 90)],
        "eyeLookOutLeft": look_out_l,
        "eyeLookInLeft": look_in_l,
        "eyeLookInRight": look_in_r,
        "eyeLookOutRight": look_out_r,
    }
    eye_keys = [(t, quat_y(d)) for t, d in gaze]

    lines = [
        "%YAML 1.1",
        "%TAG !u! tag:unity3d.com,2011:",
        "--- !u!74 &7400000",
        "AnimationClip:",
        "  m_ObjectHideFlags: 0",
        "  m_Name: ifm-replay demo",
        "  serializedVersion: 7",
        "  m_Legacy: 0",
        "  m_Compressed: 0",
        "  m_UseHighQualityCurve: 1",
        "  m_RotationCurves:",
        *rotation_curve("Armature/Head/LeftEye", eye_keys),
        *rotation_curve("Armature/Head/RightEye", eye_keys),
        "  m_CompressedRotationCurves: []",
        "  m_EulerCurves: []",
        "  m_PositionCurves: []",
        "  m_ScaleCurves: []",
        "  m_FloatCurves:",
    ]
    for name, keys in blendshapes.items():
        lines += float_curve(name, keys)
    lines += [
        "  m_PPtrCurves: []",
        "  m_SampleRate: 60",
        "  m_WrapMode: 0",
        "  m_AnimationClipSettings:",
        "    serializedVersion: 2",
        "    m_StartTime: 0",
        f"    m_StopTime: {DURATION}",
        "  m_EditorCurves: []",
        "  m_EulerEditorCurves: []",
        "  m_Events: []",
    ]
    with open(out, "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(sys.argv[1])
