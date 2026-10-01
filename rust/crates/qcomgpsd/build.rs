use std::{env, fs, path::PathBuf};
fn rust_name(name: &str) -> String {
    let mut output = String::new();
    for (index, character) in name.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 && !output.ends_with('_') {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?)
        .join("../../../openpilot/system/qcomgpsd/structs.py");
    println!("cargo:rerun-if-changed={}", source.display());
    let text = fs::read_to_string(source)?;
    let mut output =
        String::from("// Build-time declaration data; parser implementation is Rust.\n");
    for (table, name) in [
        ("gps_measurement_report", "GpsReport"),
        ("gps_measurement_report_sv", "GpsSatellite"),
        ("glonass_measurement_report", "GlonassReport"),
        ("glonass_measurement_report_sv", "GlonassSatellite"),
        ("oemdre_measurement_report", "DrReport"),
        ("oemdre_measurement_report_sv", "DrSatellite"),
        ("oemdre_svpoly_report", "SvPoly"),
        ("position_report", "Position"),
    ] {
        let marker = format!("{table} = \"\"\"");
        let body = text
            .split_once(&marker)
            .ok_or("missing declaration table")?
            .1
            .split_once("\"\"\"")
            .ok_or("missing declaration terminator")?
            .0;
        let mut fields = String::new();
        let mut values = String::new();
        let mut size = 0;
        for line in body.lines().filter(|line| !line.trim().is_empty()) {
            let mut words = line
                .split(';')
                .next()
                .ok_or("declaration line")?
                .split_whitespace();
            let typ = words.next().ok_or("missing type")?;
            let field = words.next().ok_or("missing name")?;
            let typ = if typ == "float" || field.contains("_Flt") {
                "f32"
            } else if typ == "double" || field.contains("_Dbl") {
                "f64"
            } else {
                match typ {
                    "uint8" | "uint8_t" => "u8",
                    "int8" | "int8_t" => "i8",
                    "uint16" | "uint16_t" => "u16",
                    "int16" | "int16_t" => "i16",
                    "uint32" | "uint32_t" => "u32",
                    "int32" | "int32_t" => "i32",
                    "uint64" | "uint64_t" => "u64",
                    _ => return Err("unsupported declaration type".into()),
                }
            };
            let (field, count) = match field.split_once('[') {
                Some((field, count)) => (field, count.trim_end_matches(']').parse::<usize>()?),
                None => (field, 1),
            };
            let field = rust_name(field);
            let width = match typ {
                "u8" | "i8" => 1,
                "u16" | "i16" => 2,
                "u32" | "i32" | "f32" => 4,
                "u64" | "f64" => 8,
                _ => unreachable!(),
            };
            size += count * width;
            let (declaration, value) = if count == 1 {
                (typ.to_owned(), format!("reader.{typ}()?"))
            } else {
                (
                    format!("[{typ}; {count}]"),
                    format!("[{}]", vec![format!("reader.{typ}()?"); count].join(",")),
                )
            };
            fields.push_str(&format!("pub {field}: {declaration},\n"));
            values.push_str(&format!("{field}: {value},\n"));
        }
        output.push_str(&format!("#[derive(Debug, Clone)]\npub struct {name} {{ {fields} }}\nimpl {name} {{ pub const SIZE: usize = {size}; pub fn decode(bytes: &[u8]) -> Result<Self, crate::Error> {{ let mut reader=crate::reader::Reader::new(bytes); Ok(Self {{ {values} }}) }} }}\n"));
    }
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("reports.rs"),
        output,
    )?;
    Ok(())
}
