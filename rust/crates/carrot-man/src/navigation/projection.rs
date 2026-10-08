use crate::sources::{Instruction, SafetyItem, Selection, Source};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq)]
enum Key {
    Speed(Option<SafetyItem>, Option<SafetyItem>, Option<i64>, bool),
    Guidance(Instruction),
    Receipt(bool, Option<f64>),
    Route(bool, Option<u64>, Vec<(f64, f64)>),
}
#[derive(Default)]
pub(super) struct Projector {
    session: Option<(Source, String)>,
    keys: [Option<Key>; 6],
    sequences: [u64; 6],
}
#[derive(Clone, Debug, Serialize)]
pub struct Projected {
    pub selection: Selection,
    pub session_id: String,
    pub sequences: [u64; 6],
}

impl Projector {
    pub fn project(&mut self, selection: Selection) -> Projected {
        let Some(snapshot) = &selection.snapshot else {
            return Projected {
                selection,
                session_id: String::new(),
                sequences: [0; 6],
            };
        };
        let session = (snapshot.source, snapshot.session_id.clone());
        if self.session.as_ref() != Some(&session) {
            self.session = Some(session);
            self.keys = std::array::from_fn(|_| None);
        }
        let c = &snapshot.control;
        let (primary, secondary) = if c.off_route {
            (None, None)
        } else {
            (c.safety.clone(), c.secondary_safety.clone())
        };
        let category = if [&primary, &secondary]
            .iter()
            .any(|s| s.as_ref().is_some_and(|s| s.kind == 22))
        {
            c.road_category
        } else {
            None
        };
        let keys = [
            Key::Speed(primary, secondary, category, c.off_route),
            Key::Guidance(c.current.clone()),
            Key::Guidance(c.next.clone()),
            Key::Receipt(c.position_present, c.position_received_mono_s),
            Key::Route(c.route_present, c.route_revision, c.route_points.clone()),
            Key::Receipt(c.traffic_present, c.traffic_received_mono_s),
        ];
        for (index, key) in keys.into_iter().enumerate() {
            if self.keys[index].as_ref() != Some(&key) {
                self.keys[index] = Some(key);
                self.sequences[index] += 1;
            }
        }
        Projected {
            session_id: format!("{}:{}", snapshot.source.name(), snapshot.session_id),
            selection,
            sequences: self.sequences,
        }
    }
}
