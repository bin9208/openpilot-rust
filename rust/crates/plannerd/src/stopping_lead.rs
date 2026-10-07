use crate::{number::timestamp_seconds, radar::Radar, Error};
use serde::Deserialize;

mod evidence;
use evidence::{Evidence, Observation};

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopInput {
    pub stopping: bool,
    pub speed: f64,
    pub mono_time_ns: u64,
    pub valid: bool,
}

#[derive(Debug, Default)]
pub struct StoppingLeadFilter {
    leads: [Option<Evidence>; 2],
    last_time: Option<f64>,
    pub held_mask: u8,
}

impl StoppingLeadFilter {
    pub fn update(&mut self, radar: &Radar, input: StopInput) -> Result<Radar, Error> {
        self.held_mask = 0;
        let now = timestamp_seconds(input.mono_time_ns)?;
        if !input.stopping || !input.valid || !input.speed.is_finite() || input.mono_time_ns == 0 {
            self.leads = [None, None];
            self.last_time = None;
            return Ok(*radar);
        }
        if self
            .last_time
            .is_some_and(|last| (now - last).abs() > 0.20 + 1e-6)
        {
            self.leads = [None, None];
            self.last_time = None;
        }
        let fresh = self.last_time.is_none_or(|last| now > last);
        if fresh {
            self.last_time = Some(now);
        }
        let observation = Observation {
            speed: input.speed.abs(),
            now,
            fresh,
        };
        for (index, lead) in [&radar.lead_one, &radar.lead_two].into_iter().enumerate() {
            let (evidence, held) = evidence::hold(self.leads[index].take(), lead, observation);
            self.leads[index] = evidence;
            if held {
                self.held_mask |= 1 << index;
            }
        }
        let mut output = *radar;
        for (index, lead) in [&mut output.lead_one, &mut output.lead_two]
            .into_iter()
            .enumerate()
        {
            if self.held_mask & (1 << index) != 0 {
                lead.v_lead = 0.;
                lead.v_lead_k = 0.;
                lead.a_lead = 0.;
                lead.a_lead_k = 0.;
                lead.j_lead = 0.;
            }
        }
        Ok(output)
    }
}
