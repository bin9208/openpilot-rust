use std::{env, fs, path::PathBuf};
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let source = root.join("../../../openpilot/selfdrive/carrot/server/config.py");
    println!("cargo:rerun-if-changed={}", source.display());
    let content = fs::read_to_string(source).expect("source server constants");
    let webhook = regex::Regex::new(r"(?s)DASHCAM_DEFAULT_DISCORD_WEBHOOK\s*=\s*\((.*?)\)")
        .expect("constant pattern");
    let quoted = regex::Regex::new(r#"["']([^"']*)["']"#).expect("string pattern");
    let block = webhook.captures(&content).expect("source webhook constant")[1].to_owned();
    let webhook = quoted
        .captures_iter(&block)
        .map(|capture| capture[1].to_owned())
        .collect::<String>();
    let key = regex::Regex::new(r#"DASHCAM_DEFAULT_DISCORD_KEY\s*=\s*["']([^"']*)["']"#)
        .expect("key pattern");
    let key = &key.captures(&content).expect("source webhook key")[1];
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output"));
    fs::write(
        output.join("source_defaults.rs"),
        format!("const DEFAULT_WEBHOOK: &str = {webhook:?};\nconst DEFAULT_KEY: &str = {key:?};\n"),
    )
    .expect("write generated constants");
}
