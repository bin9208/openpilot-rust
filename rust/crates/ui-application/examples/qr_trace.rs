use openpilot_ui_application::qr::{self, Correction};
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    data: String,
    correction: Correction,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin())?;
    let results = cases
        .into_iter()
        .map(|case| {
            let matrix = qr::encode(&case.data, case.correction)?;
            Ok(serde_json::json!({"size":matrix.size,"mask":matrix.mask,"modules":matrix.modules}))
        })
        .collect::<Result<Vec<_>, openpilot_ui_application::Error>>()?;
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}
