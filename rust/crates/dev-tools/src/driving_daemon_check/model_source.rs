//! Bind independently captured raw bytes to the exact compiled PKL used by both
//! native driving bundles. A catalog generation digest hashes a different object.
use crate::seal::Result;
use serde_json::Value;
use std::{os::unix::fs::FileExt, path::Path};

fn read_json(path: &Path) -> Result<Value> {
    let binding = super::Binding::open(path.to_owned(), None, 4_000_000)?;
    let mut bytes = vec![0; binding.metadata.len() as usize];
    binding.file.read_exact_at(&mut bytes, 0)?;
    binding.verify()?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn verify(catalog: &Path, original: &Value) -> Result<()> {
    let expected = original["model"]["sha256"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("original compiled model SHA-256 missing")?;
    let pointer: Value = read_json(&catalog.join("current.json"))?;
    let generation = pointer["generation"]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("catalog generation digest invalid")?;
    let directory = catalog.join(generation);
    let index: Value = read_json(&directory.join("index.json"))?;
    let bundles = index["bundles"]
        .as_array()
        .ok_or("catalog bundles missing")?;
    for camera in [[1344, 760], [1928, 1208]] {
        let matches = bundles
            .iter()
            .filter(|bundle| {
                bundle["kind"] == "driving" && bundle["camera"] == serde_json::json!(camera)
            })
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("catalog lacks unique driving camera bundle".into());
        }
        let name = matches[0]["directory"]
            .as_str()
            .filter(|name| {
                !name.is_empty()
                    && Path::new(name)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
            })
            .ok_or("catalog driving bundle directory invalid")?;
        let descriptor: Value = read_json(&directory.join(name).join("pipeline.json"))?;
        if descriptor["kind"] != "driving"
            || descriptor["camera"] != serde_json::json!(camera)
            || descriptor["sources"]["driving"] != expected
        {
            return Err(
                "original compiled model differs from native driving catalog source".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn both_camera_bundles_must_pin_the_original_compiled_model() {
        let root = tempfile::tempdir().unwrap();
        let generation = "a".repeat(64);
        let model = "b".repeat(64);
        let directory = root.path().join(&generation);
        fs::create_dir(&directory).unwrap();
        fs::write(
            root.path().join("current.json"),
            json!({"generation":generation}).to_string(),
        )
        .unwrap();
        let mut bundles = Vec::new();
        for camera in [[1344, 760], [1928, 1208]] {
            let name = format!("driving-{}x{}", camera[0], camera[1]);
            fs::create_dir(directory.join(&name)).unwrap();
            fs::write(
                directory.join(&name).join("pipeline.json"),
                json!({"kind":"driving","camera":camera,"sources":{"driving":model}}).to_string(),
            )
            .unwrap();
            bundles.push(json!({"kind":"driving","camera":camera,"directory":name}));
        }
        fs::write(
            directory.join("index.json"),
            json!({"bundles":bundles}).to_string(),
        )
        .unwrap();
        let original = json!({"model":{"sha256":model}});
        verify(root.path(), &original).unwrap();
        let second = directory.join("driving-1928x1208/pipeline.json");
        let mut descriptor: Value = serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
        descriptor["sources"]["driving"] = generation.into();
        fs::write(&second, descriptor.to_string()).unwrap();
        assert!(verify(root.path(), &original)
            .unwrap_err()
            .to_string()
            .contains("differs"));
    }
}
