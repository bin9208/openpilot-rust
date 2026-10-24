//! Original family draw data is a test oracle, never native widget policy.
use super::confirm_draw::{self, Expected};
use openpilot_dev_tools::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, process::Command, sync::OnceLock};
const COMPRESSED: &[u8] = include_bytes!("../fixtures/ui-forms/family-original-draw.json.gz");
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn corpus() -> Result<&'static str> {
    static TEXT: OnceLock<std::result::Result<String, String>> = OnceLock::new();
    match TEXT.get_or_init(|| {
        let mut text = String::new();
        flate2::read::GzDecoder::new(COMPRESSED)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        verify_text(&text).map_err(|e| e.to_string())?;
        Ok(text)
    }) {
        Ok(text) => Ok(text),
        Err(error) => Err(error.clone().into()),
    }
}
pub fn verify_text(text: &str) -> Result<()> {
    if hash(text.as_bytes()) != "8153d3789a64f41280dd9dad58202052a632535d5dcb1d2130a16cd42eca6179" {
        return Err("immutable original family draw capture changed".into());
    }
    Ok(())
}
pub fn validate(fixture: &str, case: &JsonValue) -> Result<(Value, &'static str)> {
    let text = corpus()?;
    static PARSED: OnceLock<Value> = OnceLock::new();
    let captured =
        PARSED.get_or_init(|| serde_json::from_str(text).expect("digest-checked original JSON"));
    if captured["original_sha256"]
        != "2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365"
        || captured["provenance_sha256"]
            != hash(include_bytes!("../fixtures/ui-forms/input-provenance.json"))
        || !captured["fixtures"]
            .as_object()
            .ok_or("family original fixture pins missing")?
            .values()
            .any(|v| v.as_str() == Some(hash(fixture.as_bytes()).as_str()))
    {
        return Err("family original source, fixture or provenance binding changed".into());
    }
    let name = case
        .get("name")
        .ok_or("original family name missing")?
        .to_utf8()
        .ok_or("original fixture text is not UTF-8")?;
    let original_path = captured["fixtures"]
        .as_object()
        .ok_or("family original fixture pins missing")?
        .iter()
        .find(|(_, v)| v.as_str() == Some(hash(fixture.as_bytes()).as_str()))
        .map(|(name, _)| name)
        .ok_or("original fixture digest missing")?;
    let original = JsonValue::parse(fixture)?;
    let JsonView::Array(original) = original.view() else {
        return Err("original family fixture is not a list".into());
    };
    let original_case = original
        .iter()
        .find(|v| v.get("name").and_then(|v| v.to_utf8()).as_deref() == Some(name.as_str()))
        .ok_or("original family case missing")?;
    if !confirm_draw::same_input(original_case, case) {
        return Err("original family case contract changed".into());
    }
    let trace = captured["captures"]
        .as_array()
        .ok_or("family original calls missing")?
        .iter()
        .find(|v| v["name"] == name && v["fixture"] == *original_path)
        .ok_or("original family case missing")?;
    if trace["png_sha256"].as_str()
        != Some(
            case.get("png_sha256")
                .ok_or("original raster hash missing")?
                .to_utf8()
                .ok_or("original fixture text is not UTF-8")?
                .as_str(),
        )
        || trace["exit"]
            != serde_json::from_str::<Value>(
                &case.get("exit").ok_or("original exit missing")?.to_json()?,
            )?
        || trace["state_exists"]
            != serde_json::from_str::<Value>(
                &case
                    .get("state")
                    .ok_or("original state artifact missing")?
                    .to_json()?,
            )?
    {
        return Err("original family raster, exit or artifact binding changed".into());
    }
    if let Some(raw) = case.get("scene_text") {
        // Lexical input matters for duplicate keys, non-standard floats,
        // decoder limits and surrogate escape sequences.
        if raw.to_utf8().ok_or("original fixture text is not UTF-8")?
            != trace["scene_text"].as_str().unwrap()
        {
            return Err("original raw family scene bytes changed".into());
        }
    } else {
        let captured_scene = JsonValue::parse(
            trace["scene_text"]
                .as_str()
                .ok_or("original scene bytes missing")?,
        )?;
        let expected_scene = case.get("scene").ok_or("original scene missing")?;
        if !confirm_draw::same_input(&captured_scene, &expected_scene) {
            return Err("original family scene input changed".into());
        }
    }
    if hash(trace["scene_text"].as_str().unwrap().as_bytes())
        != trace["scene_sha256"]
            .as_str()
            .ok_or("original scene digest missing")?
    {
        return Err("original family scene digest changed".into());
    }
    Ok((trace.clone(), text))
}
pub fn replay(
    fixture: &str,
    case: &JsonValue,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
) -> Result<Expected> {
    let (trace, text) = validate(fixture, case)?;
    confirm_draw::replay_trace(trace, text, root, helper, command, directory)
}
pub fn retain_failure(
    directory: &Path,
    name: &str,
    output: &Path,
    expected: &Expected,
) -> Result<()> {
    let target = directory.join(name);
    fs::create_dir_all(&target)?;
    fs::copy(output, target.join("actual.png"))?;
    fs::copy(&expected.path, target.join("expected-renderer.png"))?;
    fs::write(target.join("original-family-draw.json"), expected.capture)?;
    fs::write(target.join("expected-replay.stdout.log"), &expected.stdout)?;
    fs::write(target.join("expected-replay.stderr.log"), &expected.stderr)?;
    Ok(())
}
pub fn successful_count(fixture: &str) -> Result<usize> {
    let cases = JsonValue::parse(fixture)?;
    let JsonView::Array(cases) = cases.view() else {
        return Err("original family list missing".into());
    };
    let mut count = 0;
    for case in cases {
        if case
            .get("png")
            .is_some_and(|v| matches!(v.view(), JsonView::Bool(true)))
        {
            validate(fixture, &case)?;
            count += 1;
        }
    }
    Ok(count)
}
