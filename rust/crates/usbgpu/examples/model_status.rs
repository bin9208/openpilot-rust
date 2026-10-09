use openpilot_usbgpu::model::{status, Paths};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};
fn main() {
    for line in io::stdin().lock().lines() {
        let paths: Vec<PathBuf> = serde_json::from_str(&line.unwrap()).unwrap();
        let value = status(&Paths {
            models: paths[0].clone(),
            cache: paths[1].clone(),
            assets: paths
                .get(2)
                .cloned()
                .unwrap_or_else(|| paths[0].join("usbgpu-assets")),
        })
        .unwrap();
        println!(
            "{}",
            serde_json::json!({"compiled":value.compiled,"compile_pending":value.compile_pending})
        );
    }
}
