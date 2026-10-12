use super::{controller::Platform, params::Reader, product_id, usb::label};
use std::time::{Duration, Instant};

const HUD_CHECK: Duration = Duration::from_millis(100);
const FALLBACK_SCAN: Duration = Duration::from_secs(5);
const OFF_DIM: Duration = Duration::from_secs(30);

pub(super) fn output_allowed(
    params: &Reader,
    platform: &mut impl Platform,
    mut expected: u16,
) -> Option<u16> {
    println!("[cluster_autorun] ClusterHudDebug=0 and IsOnroad=0; keeping external HUD output off");
    let mut next_check = Instant::now();
    let mut next_dim = Instant::now();
    loop {
        if platform.interrupted() {
            return None;
        }
        let now = Instant::now();
        if now >= next_check {
            let mode = params.hud_mode();
            let Some(product) = product_id(mode) else {
                println!("[cluster_autorun] ClusterHud={mode}; stopping cluster HUD");
                return None;
            };
            expected = product;
            if params.output_allowed() {
                return Some(expected);
            }
            next_check = now + HUD_CHECK;
        }
        let now = Instant::now();
        if now >= next_dim {
            platform.dim(expected, "output disabled");
            next_dim = now + OFF_DIM;
        }
        std::thread::sleep(
            next_check
                .min(next_dim)
                .saturating_duration_since(Instant::now())
                .max(HUD_CHECK),
        );
    }
}

pub(super) fn supported_device(
    params: &Reader,
    platform: &mut impl Platform,
    mut expected: u16,
) -> Option<u16> {
    println!(
        "[cluster_autorun] {} not found or disconnected; waiting for USB event",
        label(expected)
    );
    let events = platform.events();
    if events.available() {
        println!("[cluster_autorun] fallback USB scan every 5s");
    } else {
        println!("[cluster_autorun] falling back to USB scan every 5s");
    }
    let mut next_check = Instant::now();
    let mut next_scan = Instant::now() + FALLBACK_SCAN;
    loop {
        if platform.interrupted() {
            return None;
        }
        let now = Instant::now();
        if now >= next_check {
            let mode = params.hud_mode();
            let Some(product) = product_id(mode) else {
                println!("[cluster_autorun] ClusterHud={mode}; stopping cluster HUD");
                return None;
            };
            if !params.output_allowed() {
                println!(
                    "[cluster_autorun] ClusterHudDebug=0 and IsOnroad=0; stopping HUD output while waiting for USB"
                );
                return None;
            }
            if product != expected {
                expected = product;
                next_scan = now;
            }
            next_check = now + HUD_CHECK;
        }
        let now = Instant::now();
        if now >= next_scan {
            if let Some(found) = platform.find(expected) {
                return Some(found);
            }
            next_scan = now + FALLBACK_SCAN;
        }
        let wait = next_check
            .min(next_scan)
            .saturating_duration_since(Instant::now())
            .max(HUD_CHECK);
        if events.wait(wait, expected) {
            if let Some(found) = platform.find(expected) {
                return Some(found);
            }
        }
    }
}
