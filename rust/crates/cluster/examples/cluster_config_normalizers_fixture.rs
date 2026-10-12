use openpilot_cluster::config::{normalize_cluster_panel_layout, normalize_cluster_screen_mode};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let values: Vec<String> = serde_json::from_reader(std::io::stdin().lock())?;
    let rows: Vec<_> = values
        .iter()
        .map(|value| {
            [
                normalize_cluster_screen_mode(value),
                normalize_cluster_panel_layout(value),
            ]
        })
        .collect();
    println!("{}", serde_json::to_string(&rows)?);
    Ok(())
}
