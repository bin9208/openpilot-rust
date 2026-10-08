#![cfg(feature = "native-skip-miri")]
use openpilot_pandad::firmware::native_spi::Pool;

#[test]
fn failed_initial_configuration_keeps_the_original_cached_handle() {
    let path = std::env::temp_dir().join(format!("panda-firmware-cache-{}", std::process::id()));
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap(),
    );
    let before = std::fs::read_dir("/proc/self/fd").unwrap().count();
    let pool = Pool::at_path(path.to_str().unwrap());
    assert!(pool.open(50_000_000, |_, _| Ok(())).is_err());
    assert_eq!(
        std::fs::read_dir("/proc/self/fd").unwrap().count(),
        before + 1
    );
    let cached = pool.open(50_000_000, |_, _| Ok(())).unwrap();
    assert!(cached.is_some());
    drop(cached);
    assert_eq!(
        std::fs::read_dir("/proc/self/fd").unwrap().count(),
        before + 1
    );
    drop(pool);
    assert_eq!(std::fs::read_dir("/proc/self/fd").unwrap().count(), before);
    std::fs::remove_file(path).unwrap();
}
