use openpilot_cluster::runtime_locale;
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, mode, value] = args.as_slice() else {
        return Err("cluster_locale_fixture MODE VALUE".into());
    };
    if mode == "select" {
        let mut attempted = Vec::new();
        let selected = runtime_locale::select(|candidate| {
            attempted.push(candidate);
            candidate == value
        });
        let mut environment =
            std::collections::BTreeMap::from([("LANG", "original"), ("LC_CTYPE", "original")]);
        if let Some(selected) = selected {
            for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
                environment.insert(key, selected);
            }
        }
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({"accepted":selected,"attempted":attempted,"environment":environment})
            )?
        );
    } else {
        let mut events = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(value)?;
        writeln!(
            events,
            "{}",
            serde_json::to_string(
                &serde_json::json!({"pid":std::process::id(),"lc_all":std::env::var("LC_ALL").ok()})
            )?
        )?;
        drop(events);
        runtime_locale::configure()?;
        let child = std::process::Command::new("/usr/bin/locale")
            .arg("charmap")
            .output()?;
        if !child.status.success() {
            return Err("locale child failed".into());
        }
        let environment = ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .map(|key| Ok((key, std::env::var(key)?)))
            .collect::<Result<std::collections::BTreeMap<_, _>, std::env::VarError>>()?;
        println!(
            "{}",
            serde_json::to_string(
                &serde_json::json!({"pid":std::process::id(),"environment":environment,"charmap":String::from_utf8(child.stdout)?.trim()})
            )?
        );
    }
    Ok(())
}
