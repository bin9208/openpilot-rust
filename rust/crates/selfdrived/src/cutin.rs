#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub track_id: i32,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
}

pub fn promoted(candidates: &[Candidate], lead_two: Option<Candidate>) -> Vec<Candidate> {
    let Some(lead) = lead_two.filter(|lead| lead.track_id >= 0) else {
        return Vec::new();
    };
    candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.track_id == lead.track_id
                && (candidate.d_rel - lead.d_rel).abs() <= 0.1
                && (candidate.y_rel - lead.y_rel).abs() <= 0.1
                && (candidate.v_rel - lead.v_rel).abs() <= 0.1
        })
        .collect()
}

#[derive(Default)]
pub struct Tracker {
    previous: Vec<Candidate>,
}

impl Tracker {
    pub fn previous(&self) -> &[Candidate] {
        &self.previous
    }

    pub fn update(&mut self, candidates: &[Candidate], enabled: bool) -> bool {
        let current = if enabled { candidates } else { &[] };
        let alert = current.iter().any(|candidate| {
            !self
                .previous
                .iter()
                .any(|previous| same_object(candidate, previous))
        });
        self.previous.clear();
        self.previous.extend_from_slice(current);
        alert
    }

    pub fn reset(&mut self) {
        self.previous.clear();
    }
}

fn same_object(current: &Candidate, previous: &Candidate) -> bool {
    let same_track = current.track_id >= 0 && current.track_id == previous.track_id;
    let [distance, lateral, velocity] = if same_track {
        [3.0, 1.0, 5.0]
    } else {
        [1.5, 0.75, 2.5]
    };
    (current.d_rel - previous.d_rel).abs() <= distance
        && (current.y_rel - previous.y_rel).abs() <= lateral
        && (current.v_rel - previous.v_rel).abs() <= velocity
}
