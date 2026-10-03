fn main() {
    match openpilot_camera_kernel::Master::open() {
        Ok(master) => println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "device": master.mmu.device,
                "cdm": master.mmu.cdm,
                "icp": master.mmu.icp,
                "subscription": master.subscription.code,
            })
        ),
        Err(error) => {
            eprintln!("{error}");
            println!("{}", serde_json::json!({"ok": false}));
            std::process::exit(1);
        }
    }
}
