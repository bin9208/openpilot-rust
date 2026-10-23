// Included inside the production framing module so this regression can measure
// retained bytes without adding test-only inspection methods to the runtime API.
use super::Frames;

#[test]
fn delimiter_free_retention_is_bounded() {
    let mut frames = Frames::default();
    // Repeated small transfers reproduce the real serial recv loop. The old
    // parser grows by another transfer on every iteration, without a delimiter.
    let transfer = [0x7d; 4096];
    for _ in 0..128 {
        frames.extend(&transfer);
        assert!(frames.next_frame().unwrap().is_none());
        assert!(
            frames.pending.len() <= 65_539,
            "delimiter-free bytes retained: {}",
            frames.pending.len()
        );
        assert!(frames.pending.capacity() <= 131_078);
    }
}
