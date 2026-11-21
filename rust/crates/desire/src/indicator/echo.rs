//! Bounded recognition of our one-touch echo, never a general stalk mask.
//! Port of carrot's `desire_lib/indicator_feedback.py`.
use crate::types::Car;
use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct TapEcho {
    ident: Option<String>,
    since: Option<f64>,
    finished: bool,
}

impl TapEcho {
    pub fn matches(
        &mut self,
        now: f64,
        car: &Car,
        direction: u8,
        request_time: f64,
        value: Option<&Value>,
    ) -> bool {
        value
            .and_then(|value| self.inner(now, car, direction, request_time, value))
            .unwrap_or(false)
    }

    fn inner(
        &mut self,
        now: f64,
        car: &Car,
        direction: u8,
        request_time: f64,
        value: &Value,
    ) -> Option<bool> {
        let tap = value.get("tap")?;
        let id = tap.get("id")?.as_str()?;
        let stamp = value.get("stamp")?.as_f64()?;
        let start = tap.get("start")?.as_f64()?;
        let request = tap.get("request")?.as_f64()?;
        if id.is_empty()
            || tap.get("direction")?.as_i64()? != i64::from(direction)
            || (request - request_time).abs() > 1e-6
            || ![now, stamp, start, request].iter().all(|v| v.is_finite())
            || !(0. ..0.15).contains(&(now - stamp))
            || !(0. ..0.6).contains(&(now - start))
        {
            return Some(false);
        }
        if self.ident.as_deref() != Some(id) {
            self.ident = Some(id.to_owned());
            self.since = None;
            self.finished = false;
        }
        if car.blinker_lever == 0 {
            return Some(false);
        }
        let counts = [
            i64::from(car.stalk_counts[0]),
            i64::from(car.stalk_counts[1]),
        ];
        let baseline: Vec<i64> = tap
            .get("counts")?
            .as_array()?
            .iter()
            .map(Value::as_i64)
            .collect::<Option<_>>()?;
        let ok = matches!(direction, 1 | 2) && baseline.len() == 2 && {
            let i = usize::from(direction - 1);
            let limit = if direction == 1 { 3 } else { 4 };
            !self.finished
                && car.blinker_lever == 1
                && baseline.iter().all(|x| (0..256).contains(x))
                && (1..=limit).contains(&(counts[i] - baseline[i]).rem_euclid(256))
                && counts[1 - i] == baseline[1 - i]
                && !(if direction == 1 {
                    car.right_blinker
                } else {
                    car.left_blinker
                })
        };
        if !ok {
            self.finished = true;
            return Some(false);
        }
        let since = *self.since.get_or_insert(now);
        Some((0. ..0.25).contains(&(now - since)))
    }
}
