//! Four bounded independent fixes; source selection never renews receipt time.
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Source {
    ExternalNavi,
    Phone,
    Vehicle,
    Device,
}
impl Source {
    pub const fn name(self) -> &'static str {
        match self {
            Self::ExternalNavi => "external_navi",
            Self::Phone => "phone",
            Self::Vehicle => "vehicle",
            Self::Device => "device",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Fix {
    pub latitude: f64,
    pub longitude: f64,
    pub heading: Option<f64>,
    pub speed: f64,
    pub stamp: f64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Sources {
    fixes: [Option<Fix>; 4],
}
impl Sources {
    pub fn remove(&mut self, source: Source) {
        self.fixes[source as usize] = None;
    }
    pub fn put(&mut self, source: Source, mut fix: Fix) {
        if ![fix.latitude, fix.longitude, fix.speed, fix.stamp]
            .iter()
            .all(|v| v.is_finite())
            || !(-85.05112878..=85.05112878).contains(&fix.latitude)
            || !(-180. ..=180.).contains(&fix.longitude)
            || (fix.latitude == 0. && fix.longitude == 0.)
            || fix.stamp <= 0.
            || !(0. ..=90.).contains(&fix.speed)
        {
            self.remove(source);
            return;
        }
        fix.heading = fix
            .heading
            .filter(|v| v.is_finite() && (0. ..360.).contains(v));
        self.fixes[source as usize] = Some(fix);
    }
    pub fn select(&self, now: f64) -> Option<(Source, Fix)> {
        for source in [
            Source::ExternalNavi,
            Source::Phone,
            Source::Vehicle,
            Source::Device,
        ] {
            if let Some(fix) = self.fixes[source as usize] {
                let timeout = if source == Source::Vehicle { 2.5 } else { 3. };
                if (0. ..timeout).contains(&(now - fix.stamp)) {
                    return Some((source, fix));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_priority_expires_original_receipts_without_renewal() {
        let mut sources = Sources::default();
        for name in [
            Source::Device,
            Source::Vehicle,
            Source::Phone,
            Source::ExternalNavi,
        ] {
            sources.put(
                name,
                Fix {
                    latitude: 37.,
                    longitude: 127.,
                    heading: Some(90.),
                    speed: 10.,
                    stamp: 10.,
                },
            );
        }
        assert_eq!(sources.select(11.).unwrap().0, Source::ExternalNavi);
        sources.remove(Source::ExternalNavi);
        assert_eq!(sources.select(11.).unwrap().0, Source::Phone);
        sources.remove(Source::Phone);
        assert_eq!(sources.select(12.499).unwrap().0, Source::Vehicle);
        assert_eq!(sources.select(12.5).unwrap().0, Source::Device);
        assert!(sources.select(13.).is_none());
        assert!(sources.select(9.).is_none());
    }
    #[test]
    fn invalid_fix_removes_only_its_source_and_heading_is_optional() {
        let mut sources = Sources::default();
        let mut fix = Fix {
            latitude: 37.,
            longitude: 127.,
            heading: Some(f64::NAN),
            speed: 0.,
            stamp: 1.,
        };
        sources.put(Source::Phone, fix);
        assert_eq!(sources.select(1.).unwrap().1.heading, None);
        for (latitude, longitude, speed, stamp) in [
            (0., 0., 0., 1.),
            (86., 127., 0., 1.),
            (37., 181., 0., 1.),
            (37., 127., 91., 1.),
            (37., 127., 0., 0.),
            (f64::NAN, 127., 0., 1.),
        ] {
            fix.latitude = latitude;
            fix.longitude = longitude;
            fix.speed = speed;
            fix.stamp = stamp;
            sources.put(Source::Phone, fix);
            assert!(sources.select(1.).is_none());
        }
    }
}
