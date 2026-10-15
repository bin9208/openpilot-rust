//! Suspension requires explicit, current route metadata; absent categories do not count.
use super::transport::Service;
#[derive(Debug, Default)]
pub struct RoadPolicy<'a> {
    pub service: Service,
    pub schema_version: u16,
    pub connected: bool,
    pub session_id: &'a str,
    pub navigation_present: bool,
    pub off_route: bool,
    pub lane_present: bool,
    pub road_category_valid: bool,
    pub road_category: u8,
    pub received_mono_ns: u64,
}
impl RoadPolicy<'_> {
    pub fn suspended(&self, now: f64) -> bool {
        self.service.fresh(now, 2.)
            && self.schema_version == 1
            && self.connected
            && !self.session_id.is_empty()
            && !(self.navigation_present && self.off_route)
            && self.lane_present
            && self.road_category_valid
            && matches!(self.road_category, 0 | 1)
            && self.received_mono_ns > 0
            && (0. ..=10.).contains(&(now - self.received_mono_ns as f64 / 1e9))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_fresh_category_required_and_route_departure_cancels() {
        let mut policy = RoadPolicy {
            service: Service {
                seen: true,
                alive: true,
                valid: true,
                receive_time: 20.,
            },
            schema_version: 1,
            connected: true,
            session_id: "route",
            lane_present: true,
            road_category_valid: true,
            received_mono_ns: 19_000_000_000,
            ..RoadPolicy::default()
        };
        assert!(policy.suspended(20.));
        policy.road_category_valid = false;
        assert!(!policy.suspended(20.));
        policy.road_category_valid = true;
        policy.off_route = true;
        assert!(policy.suspended(20.));
        policy.navigation_present = true;
        assert!(!policy.suspended(20.));
        policy.off_route = false;
        assert!(!policy.suspended(22.01));
    }
}
