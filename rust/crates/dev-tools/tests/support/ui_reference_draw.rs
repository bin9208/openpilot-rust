//! Original Forms, Render and Network draw data is a test oracle, never native widget policy.
use super::confirm_draw::{self, Expected};
use openpilot_dev_tools::seal::Result;
use openpilot_log_format::{JsonValue, JsonView};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, process::Command, sync::OnceLock};
const COMPRESSED: &[u8] = include_bytes!("../fixtures/ui-forms/reference-original-draw.json.gz");
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
    if hash(text.as_bytes()) != "0b3de25ac1f8a8da32c0daa4d1137b78e0cfc6a1cc1a6879c3173ebcb3e9c604" {
        return Err("immutable original reference draw capture changed".into());
    }
    Ok(())
}
pub fn validate(fixture: &str, case: &JsonValue) -> Result<(Value, &'static str)> {
    let text = corpus()?;
    static PARSED: OnceLock<Value> = OnceLock::new();
    let captured =
        PARSED.get_or_init(|| serde_json::from_str(text).expect("digest-checked original JSON"));
    let source_pins = [
        (
            "ui-forms",
            "2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365",
            include_bytes!("../fixtures/ui-forms/input-provenance.json").as_slice(),
        ),
        (
            "ui-render",
            "257dfe56f49f0c2ae1d007305126e69ab858d14ccf8e586cfeea30d29431be0c",
            include_bytes!("../fixtures/ui-render/provenance.json").as_slice(),
        ),
        (
            "ui-network",
            "00fb81b7a01eb5b515d9af98c03a7a5a1b7ab25f52f9ffba4d335034e03163b9",
            include_bytes!("../fixtures/ui-network/provenance.json").as_slice(),
        ),
    ];
    for (group, source, provenance) in source_pins {
        if captured["sources"][group] != source || captured["provenance"][group] != hash(provenance)
        {
            return Err("original source or provenance binding changed".into());
        }
    }
    let name = case
        .get("name")
        .ok_or("original reference name missing")?
        .to_utf8()
        .ok_or("original fixture text is not UTF-8")?;
    let original_path = captured["fixtures"]
        .as_object()
        .ok_or("reference original fixture pins missing")?
        .iter()
        .find(|(_, v)| v.as_str() == Some(hash(fixture.as_bytes()).as_str()))
        .map(|(name, _)| name)
        .ok_or("original fixture digest missing")?;
    let original = JsonValue::parse(fixture)?;
    let JsonView::Array(original) = original.view() else {
        return Err("original reference fixture is not a list".into());
    };
    let original_case = original
        .iter()
        .find(|v| v.get("name").and_then(|v| v.to_utf8()).as_deref() == Some(name.as_str()))
        .ok_or("original reference case missing")?;
    if !confirm_draw::same_input(original_case, case) {
        return Err("original reference case contract changed".into());
    }
    let trace = captured["captures"]
        .as_array()
        .ok_or("reference original calls missing")?
        .iter()
        .find(|v| v["name"] == name && v["fixture"] == *original_path)
        .ok_or("original reference case missing")?;
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
                    .map(|v| v.to_json())
                    .transpose()?
                    .unwrap_or_else(|| "true".into()),
            )?
    {
        return Err("original reference raster, exit or artifact binding changed".into());
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
            return Err("original reference scene input changed".into());
        }
    }
    if hash(trace["scene_text"].as_str().unwrap().as_bytes())
        != trace["scene_sha256"]
            .as_str()
            .ok_or("original scene digest missing")?
    {
        return Err("original reference scene digest changed".into());
    }
    Ok((trace.clone(), text))
}
/// Existing serde-based tests keep their historical state assertions, but
/// capture binding uses the original lossless numbers rather than reserialized
/// serde floats. Reject caller mutations before selecting the lossless row.
pub fn from_serde_case(fixture: &str, case: &Value) -> Result<JsonValue> {
    let original: Value = serde_json::from_str(fixture)?;
    if !original
        .as_array()
        .ok_or("original fixture list missing")?
        .iter()
        .any(|v| v == case)
    {
        return Err("serde fixture case changed".into());
    }
    let lossless = JsonValue::parse(fixture)?;
    let JsonView::Array(cases) = lossless.view() else {
        return Err("lossless fixture list missing".into());
    };
    cases
        .into_iter()
        .find(|v| v.get("name").and_then(|n| n.to_utf8()).as_deref() == case["name"].as_str())
        .ok_or_else(|| "lossless fixture case missing".into())
}
pub fn scene_bytes(fixture: &str, case: &Value) -> Result<Vec<u8>> {
    let original = from_serde_case(fixture, case)?;
    if let Some(raw) = original.get("scene_text") {
        return Ok(raw
            .to_utf8()
            .ok_or("raw original scene is not UTF8")?
            .into_bytes());
    }
    Ok(original
        .get("scene")
        .ok_or("original scene missing")?
        .to_json()?
        .into_bytes())
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
        return Err("original reference list missing".into());
    };
    let mut count = 0;
    for case in cases {
        if case.get("png_sha256").is_some() {
            validate(fixture, &case)?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn assert_png(
    fixture: &str,
    case: &JsonValue,
    root: &Path,
    helper: &Path,
    command: &Command,
    directory: &Path,
    output: &Path,
) -> Expected {
    let expected = replay(fixture, case, root, helper, command, directory).unwrap();
    let actual = hash(&fs::read(output).unwrap());
    if actual != expected.hash {
        if let Some(destination) = std::env::var_os("UI_FORMS_FAILURE_OUTPUT") {
            let name = case.get("name").unwrap().to_utf8().unwrap();
            let namespace = Path::new(&destination)
                .join("reference")
                .join(&hash(fixture.as_bytes())[..12]);
            retain_failure(&namespace, &name, output, &expected).unwrap();
            let target = namespace.join(name);
            fs::write(target.join("expected-case.json"), case.to_json().unwrap()).unwrap();
            let state = output.with_extension("json");
            if state.exists() {
                fs::copy(state, target.join("actual-state.json")).unwrap();
            }
            let input = directory.join("scene.json");
            if input.exists() {
                fs::copy(input, target.join("scene.json")).unwrap();
            }
            fs::write(
                target.join("replay-environment.txt"),
                format!("{command:?}"),
            )
            .unwrap();
        }
    }
    assert_eq!(
        actual,
        expected.hash,
        "{} exact original draw replay",
        case.get("name").unwrap().to_json().unwrap()
    );
    expected
}
pub fn check_complete(root: &Path) -> Result<usize> {
    let value: Value = serde_json::from_str(corpus()?)?;
    let fixtures = value["fixtures"]
        .as_object()
        .ok_or("original fixture set missing")?;
    if fixtures.len() != 34 {
        return Err("original fixture set incomplete".into());
    }
    let mut count = 0;
    for path in fixtures.keys() {
        let fixture =
            fs::read_to_string(root.join("rust/crates/dev-tools/tests/fixtures").join(path))?;
        count += successful_count(&fixture)?;
    }
    if count != 243 {
        return Err("original raster capture coverage incomplete".into());
    }
    Ok(count)
}
