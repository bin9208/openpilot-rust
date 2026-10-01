use openpilot_ui_framework::multilang::{parse, plural_index};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root] = args.as_slice() else {
        return Err("usage: translation_trace TRANSLATIONS".into());
    };
    let root = std::path::Path::new(root);
    let mut output = serde_json::Map::new();
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "po") {
            output.insert(
                path.file_name()
                    .ok_or("missing filename")?
                    .to_string_lossy()
                    .into_owned(),
                serde_json::to_value(parse(&std::fs::read_to_string(path)?)?)?,
            );
        }
    }
    let mut plurals = serde_json::Map::new();
    for lang in [
        "en", "de", "fr", "pt-BR", "es", "tr", "uk", "th", "zh-CHT", "zh-CHS", "ko", "ja",
        "missing",
    ] {
        plurals.insert(
            lang.to_owned(),
            serde_json::to_value(
                (-220..=220)
                    .map(|n| plural_index(lang, n))
                    .collect::<Vec<_>>(),
            )?,
        );
    }
    output.insert("selectors".to_owned(), plurals.into());
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
