//! Reads a text-serialized Unity `.anim` clip: blendshape curves and bone rotation curves.

use crate::curve::{Curve, Key};
use anyhow::{anyhow, bail, Context, Result};
use std::collections::BTreeMap;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Clone, Debug, PartialEq)]
pub enum Rotation {
    /// x, y, z, w in Unity's local space.
    Quaternion([Curve; 4]),
    /// x, y, z in degrees, as Unity's Euler curves store them.
    Euler([Curve; 3]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoneCurve {
    pub path: String,
    pub rotation: Rotation,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub name: String,
    pub duration: f64,
    /// Blendshape name (without the `blendShape.` prefix) and its 0–100 weight curve.
    pub blendshapes: Vec<(String, Curve)>,
    pub bones: Vec<BoneCurve>,
}

pub fn parse(text: &str) -> Result<Clip> {
    // Unity's tag directive and `!u!74 &7400000` document tags are not needed to read the data.
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with('%'))
        .map(|line| if line.starts_with("--- ") { "---" } else { line })
        .collect::<Vec<_>>()
        .join("\n");
    let docs = YamlLoader::load_from_str(&body).context("not a Unity YAML file")?;
    let clip = docs
        .iter()
        .map(|doc| &doc["AnimationClip"])
        .find(|clip| !clip.is_badvalue())
        .ok_or_else(|| anyhow!("no AnimationClip in this file"))?;

    let mut blendshapes = blendshape_curves(&clip["m_FloatCurves"]);
    if blendshapes.is_empty() {
        blendshapes = blendshape_curves(&clip["m_EditorCurves"]);
    }
    let mut bones: Vec<BoneCurve> = vector_curves(&clip["m_RotationCurves"], &["x", "y", "z", "w"])
        .into_iter()
        .map(|(path, c)| BoneCurve { path, rotation: Rotation::Quaternion(into_array(c)) })
        .chain(
            vector_curves(&clip["m_EulerCurves"], &["x", "y", "z"])
                .into_iter()
                .map(|(path, c)| BoneCurve { path, rotation: Rotation::Euler(into_array(c)) }),
        )
        .collect();
    if bones.is_empty() {
        bones = editor_rotations(&clip["m_EditorCurves"]);
    }
    if blendshapes.is_empty() && bones.is_empty() {
        bail!("this clip has no blendshape or rotation curves");
    }

    let stop = number(&clip["m_AnimationClipSettings"]["m_StopTime"]).filter(|t| *t > 0.);
    let last_key = blendshapes
        .iter()
        .map(|(_, c)| c.end_time())
        .chain(bones.iter().flat_map(|b| match &b.rotation {
            Rotation::Quaternion(c) => c.iter().map(Curve::end_time).collect::<Vec<_>>(),
            Rotation::Euler(c) => c.iter().map(Curve::end_time).collect(),
        }))
        .fold(0., f64::max);

    Ok(Clip {
        name: clip["m_Name"].as_str().unwrap_or_default().to_string(),
        duration: stop.unwrap_or(last_key),
        blendshapes,
        bones,
    })
}

/// Unity writes numbers as YAML floats or ints, and infinite tangents as `Infinity`.
fn number(value: &Yaml) -> Option<f64> {
    match value {
        Yaml::Real(s) | Yaml::String(s) => s.parse().ok(),
        Yaml::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn entries(list: &Yaml) -> &[Yaml] {
    list.as_vec().map_or(&[], Vec::as_slice)
}

fn keys(entry: &Yaml, component: Option<&str>) -> Curve {
    let pick = |v: &Yaml| number(component.map_or(v, |c| &v[c]));
    Curve::new(
        entries(&entry["curve"]["m_Curve"])
            .iter()
            .filter_map(|k| {
                Some(Key {
                    time: number(&k["time"])?,
                    value: pick(&k["value"])?,
                    in_slope: pick(&k["inSlope"]).unwrap_or(0.),
                    out_slope: pick(&k["outSlope"]).unwrap_or(0.),
                })
            })
            .collect(),
    )
}

fn blendshape_curves(list: &Yaml) -> Vec<(String, Curve)> {
    let mut seen = BTreeMap::new();
    entries(list)
        .iter()
        .filter_map(|entry| {
            // Unity writes `blendShape.` for a mesh import and `BlendShape.` for the iFacialMocap FBX.
            let attribute = entry["attribute"].as_str()?;
            let prefix = "blendShape.".len();
            let name = attribute
                .get(..prefix)
                .filter(|p| p.eq_ignore_ascii_case("blendShape."))
                .map(|_| &attribute[prefix..])?;
            // The same shape on two meshes animates the same face channel; keep the first.
            seen.insert(name.to_string(), ()).is_none().then(|| (name.to_string(), keys(entry, None)))
        })
        .collect()
}

fn vector_curves(list: &Yaml, components: &[&str]) -> Vec<(String, Vec<Curve>)> {
    entries(list)
        .iter()
        .map(|entry| {
            let path = entry["path"].as_str().unwrap_or_default().to_string();
            (path, components.iter().map(|c| keys(entry, Some(c))).collect())
        })
        .collect()
}

/// Rebuilds rotations from the per-component editor curves, for clips that only keep those.
fn editor_rotations(list: &Yaml) -> Vec<BoneCurve> {
    let mut quats: BTreeMap<String, [Curve; 4]> = BTreeMap::new();
    let mut eulers: BTreeMap<String, [Curve; 3]> = BTreeMap::new();
    for entry in entries(list) {
        let (Some(attribute), Some(path)) = (entry["attribute"].as_str(), entry["path"].as_str())
        else {
            continue;
        };
        let Some((property, axis)) = attribute.rsplit_once('.') else { continue };
        let index = "xyzw".find(axis);
        match (property, index) {
            ("m_LocalRotation", Some(i)) => {
                quats.entry(path.into()).or_default()[i] = keys(entry, None);
            }
            ("localEulerAnglesRaw" | "localEulerAngles", Some(i)) if i < 3 => {
                eulers.entry(path.into()).or_default()[i] = keys(entry, None);
            }
            _ => {}
        }
    }
    quats
        .into_iter()
        .map(|(path, c)| BoneCurve { path, rotation: Rotation::Quaternion(c) })
        .chain(eulers.into_iter().map(|(path, c)| BoneCurve { path, rotation: Rotation::Euler(c) }))
        .collect()
}

fn into_array<const N: usize>(curves: Vec<Curve>) -> [Curve; N] {
    curves.try_into().unwrap_or_else(|_| std::array::from_fn(|_| Curve::default()))
}
