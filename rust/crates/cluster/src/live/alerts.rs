use crate::models::ClusterAlert;

#[derive(Default)]
pub(super) struct Lifecycle {
    onroad: bool,
    started: Option<f64>,
    seen: bool,
}
#[derive(Clone, Copy)]
pub(super) struct Receipt {
    pub received: Option<f64>,
    pub updated: bool,
    pub enabled: bool,
}

impl Lifecycle {
    pub fn update(
        &mut self,
        current: Option<ClusterAlert>,
        onroad: bool,
        now: f64,
        receipt: Receipt,
    ) -> Option<ClusterAlert> {
        if !onroad {
            self.onroad = false;
            self.started = None;
            self.seen = false;
            return current;
        }
        if !self.onroad {
            self.started = Some(now);
            self.seen = false;
        }
        self.onroad = true;
        let started = *self.started.get_or_insert(now);
        if receipt.updated || receipt.received.is_some_and(|received| received >= started) {
            self.seen = true;
        }
        if !self.seen {
            return (now - started > 5.0).then(|| {
                alert(
                    "openpilot Unavailable",
                    "Waiting to start",
                    2,
                    0,
                    "clusterSelfdriveStartup",
                )
            });
        }
        if let Some(received) = receipt.received {
            let missing = (now - received).max(0.0);
            if missing > 5.0 {
                return Some(if receipt.enabled && missing - 5.0 < 10.0 {
                    alert(
                        "TAKE CONTROL IMMEDIATELY",
                        "System Unresponsive",
                        3,
                        2,
                        "clusterSelfdriveTimeout",
                    )
                } else {
                    alert(
                        "System Unresponsive",
                        "Reboot Device",
                        2,
                        0,
                        "clusterSelfdriveReboot",
                    )
                });
            }
        }
        current
    }
}

fn alert(text1: &str, text2: &str, size: i64, status: i64, alert_type: &str) -> ClusterAlert {
    ClusterAlert {
        text1: text1.into(),
        text2: text2.into(),
        size,
        status,
        alert_type: alert_type.into(),
    }
}
