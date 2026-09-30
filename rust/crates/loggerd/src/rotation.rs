#[derive(Default)]
pub struct Rotation {
    pub ready: usize,
    pub last_rotation_ns: u64,
    pub last_camera_ns: u64,
    pub test_mode: bool,
}

impl Rotation {
    pub fn due(&self, now_ns: u64) -> bool {
        let duration = now_ns.saturating_sub(self.last_rotation_ns);
        self.ready == 4
            || (!self.test_mode
                && duration > 60_000_000_000
                && (now_ns.saturating_sub(self.last_camera_ns) > 500_000_000
                    || duration > 72_000_000_000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_when_all_four_encoders_are_ready() {
        let state = Rotation {
            ready: 4,
            ..Rotation::default()
        };
        let due = state.due(0);
        assert!(due);
    }

    #[test]
    fn rotates_only_beyond_source_timeout_boundaries() {
        for (now_ns, camera_ns, expected) in [
            (60_000_000_000, 0, false),
            (60_000_000_001, 0, true),
            (61_000_000_000, 60_500_000_000, false),
            (61_000_000_001, 60_500_000_000, true),
            (72_000_000_000, 72_000_000_000, false),
            (72_000_000_001, 72_000_000_001, true),
        ] {
            let state = Rotation {
                last_camera_ns: camera_ns,
                ..Rotation::default()
            };
            let due = state.due(now_ns);
            assert_eq!(due, expected, "now={now_ns}, camera={camera_ns}");
        }
    }

    #[test]
    fn waits_for_encoders_when_source_test_mode_disables_timeout() {
        let state = Rotation {
            test_mode: true,
            ..Rotation::default()
        };
        let due = state.due(1_000_000_000_000);
        assert!(!due);
    }
}
