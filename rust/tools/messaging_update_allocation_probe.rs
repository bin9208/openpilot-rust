//! Measure the actual isolated SubMaster::update path, then compare receipts.
//! Compile this unchanged source against baseline and candidate messaging crates.
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::{
    runtime::{PubMaster, SubMaster},
    state::Options,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    error::Error,
    fs,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Count;
fn count(size: usize) {
    if TRACK.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
    }
}
// SAFETY: the probe forwards every pointer and layout unchanged to System.
// Atomic counters allocate nothing; bytes are requested allocation sizes.
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;

const WARMUP: usize = 10;
const UPDATES: usize = 1_000;
const HEADER: &str = "messaging-update-allocation-v1";
const ROSTER: [&str; 4] = ["empty", "single", "two", "malformed"];

#[derive(Clone, Copy)]
struct Receipt {
    updates: usize,
    calls: usize,
    bytes: usize,
    fingerprint: u64,
}
fn packet(name: &str, stamp: u64) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(stamp);
    root.set_valid(true);
    match name {
        "carState" => {
            root.init_car_state().set_v_ego(3.0);
        }
        "deviceState" => {
            root.init_device_state();
        }
        _ => unreachable!(),
    }
    capnp::serialize::write_message_to_words(&message)
}
fn mix(mut hash: u64, value: u64) -> u64 {
    for byte in value.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
fn fingerprint(
    subscriber: &SubMaster,
    names: &[&str],
    mut hash: u64,
) -> Result<u64, Box<dyn Error>> {
    hash = mix(hash, subscriber.state.frame() as u64);
    for name in names {
        let topic = subscriber.state.topic(name)?;
        hash = mix(hash, u64::from(topic.updated));
        hash = mix(hash, u64::from(topic.valid));
        hash = mix(hash, topic.log_mono_time);
    }
    Ok(hash)
}
fn namespace(name: &str) -> Result<String, Box<dyn Error>> {
    let prefix = format!("rust-probe-messagingalloc-{name}-{}", std::process::id());
    let directory = format!("/dev/shm/msgq_{prefix}");
    fs::create_dir(&directory)?;
    std::env::set_var("OPENPILOT_PREFIX", prefix);
    std::env::remove_var("CEREAL_FAKE");
    Ok(directory)
}
fn measure(name: &str) -> Result<Receipt, Box<dyn Error>> {
    let directory = namespace(name)?;
    let names: &[&str] = if name == "two" {
        &["carState", "deviceState"]
    } else {
        &["carState"]
    };
    let mut publisher = PubMaster::isolated(names)?;
    let mut subscriber = SubMaster::isolated(names, Options::default())?;
    let car = packet("carState", 17);
    let device = packet("deviceState", 23);
    let mut calls = 0;
    let mut bytes = 0;
    let mut hash = 0xcbf29ce484222325;
    for index in 0..WARMUP + UPDATES {
        if name != "empty" {
            publisher.send("carState", &car)?;
            if name == "two" {
                publisher.send("deviceState", &device)?;
            }
        }
        if index >= WARMUP {
            CALLS.store(0, Ordering::Relaxed);
            BYTES.store(0, Ordering::Relaxed);
            TRACK.store(true, Ordering::Relaxed);
        }
        let result = subscriber.update(Duration::ZERO);
        TRACK.store(false, Ordering::Relaxed);
        result?;
        if index >= WARMUP {
            calls += CALLS.load(Ordering::Relaxed);
            bytes += BYTES.load(Ordering::Relaxed);
            for topic in names {
                let topic = subscriber.state.topic(topic)?;
                assert_eq!(topic.updated, name != "empty");
                assert_eq!(topic.valid, name != "empty");
            }
            hash = fingerprint(&subscriber, names, hash)?;
        }
    }
    drop(subscriber);
    drop(publisher);
    fs::remove_dir_all(directory)?;
    Ok(Receipt {
        updates: UPDATES,
        calls,
        bytes,
        fingerprint: hash,
    })
}
fn malformed() -> Result<Receipt, Box<dyn Error>> {
    let directory = namespace("malformed")?;
    let names = ["carState", "deviceState"];
    let mut publisher = PubMaster::isolated(&names)?;
    let mut subscriber = SubMaster::isolated(&names, Options::default())?;
    publisher.send("carState", &packet("carState", 17))?;
    publisher.send(
        "deviceState",
        &[0_u32.to_le_bytes(), 100_u32.to_le_bytes()].concat(),
    )?;
    assert!(subscriber.update(Duration::ZERO).is_err());
    assert_eq!(subscriber.state.frame(), -1);
    for name in names {
        let topic = subscriber.state.topic(name)?;
        assert!(!topic.updated && !topic.valid);
        assert_eq!(topic.log_mono_time, 0);
    }
    let fingerprint = fingerprint(&subscriber, &names, 0xcbf29ce484222325)?;
    drop(subscriber);
    drop(publisher);
    fs::remove_dir_all(directory)?;
    Ok(Receipt {
        updates: 1,
        calls: 0,
        bytes: 0,
        fingerprint,
    })
}
fn render(receipts: &[Receipt; 4]) -> String {
    let mut text = format!("{HEADER}\n");
    for (name, receipt) in ROSTER.into_iter().zip(receipts) {
        text.push_str(&format!(
            "{name}\t{}\t{}\t{}\t{:016x}\n",
            receipt.updates, receipt.calls, receipt.bytes, receipt.fingerprint
        ));
    }
    text
}
fn parse(text: &str) -> Result<[Receipt; 4], Box<dyn Error>> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("wrong receipt header".into());
    }
    let mut receipts = [Receipt {
        updates: 0,
        calls: 0,
        bytes: 0,
        fingerprint: 0,
    }; 4];
    for (name, receipt) in ROSTER.into_iter().zip(&mut receipts) {
        let line = lines.next().ok_or("missing receipt row")?;
        let mut fields = line.split('\t');
        if fields.next() != Some(name) {
            return Err("wrong receipt row".into());
        }
        receipt.updates = fields.next().ok_or("missing count")?.parse()?;
        receipt.calls = fields.next().ok_or("missing calls")?.parse()?;
        receipt.bytes = fields.next().ok_or("missing bytes")?.parse()?;
        receipt.fingerprint = u64::from_str_radix(fields.next().ok_or("missing fingerprint")?, 16)?;
        if fields.next().is_some()
            || receipt.updates != if name == "malformed" { 1 } else { UPDATES }
            || (name == "malformed" && (receipt.calls != 0 || receipt.bytes != 0))
        {
            return Err("invalid receipt fields".into());
        }
    }
    if lines.next().is_some() || !text.ends_with('\n') {
        return Err("unexpected receipt tail".into());
    }
    Ok(receipts)
}
fn compare(old: &str, current: &str) -> Result<(), Box<dyn Error>> {
    let old = parse(old)?;
    let current = parse(current)?;
    for (name, (before, after)) in ROSTER.into_iter().zip(old.iter().zip(&current)) {
        if before.fingerprint != after.fingerprint {
            return Err(format!("{name} semantic fingerprint changed").into());
        }
    }
    if old[0].calls != current[0].calls || old[0].bytes != current[0].bytes {
        return Err("empty-update allocation control changed".into());
    }
    for index in [1, 2] {
        if current[index].calls >= old[index].calls || current[index].bytes >= old[index].bytes {
            return Err(format!("{} did not improve in calls and bytes", ROSTER[index]).into());
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 4 && args[1] == "--compare" {
        compare(
            &fs::read_to_string(&args[2])?,
            &fs::read_to_string(&args[3])?,
        )?;
        println!("messaging-update-allocation comparison passed");
        return Ok(());
    }
    if args.len() != 1 {
        return Err("usage: messaging_update_allocation_probe [--compare OLD CURRENT]".into());
    }
    print!(
        "{}",
        render(&[
            measure("empty")?,
            measure("single")?,
            measure("two")?,
            malformed()?
        ])
    );
    Ok(())
}
