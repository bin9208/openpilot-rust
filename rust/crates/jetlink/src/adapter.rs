use crate::{contract, Deadline, Error};
use openpilot_modeld::{parse::RawOutputs, prediction::DrivingPrediction};
use std::collections::BTreeMap;

pub struct Adapter {
    previous_desire: [f32; 8],
    reset_next: bool,
    slices: BTreeMap<String, [usize; 2]>,
}
impl Adapter {
    pub fn new() -> Result<Self, Error> {
        let mut slices: BTreeMap<String, [usize; 2]> =
            serde_json::from_value(contract::contract()?["output_slices"].clone())?;
        // Original get_action consumes action[0,0:2]; the remaining raw values are retained on the wire.
        slices.insert("action".to_owned(), [2062, 2064]);
        Ok(Self {
            previous_desire: [0.0; 8],
            reset_next: true,
            slices,
        })
    }
    pub fn reset(&mut self) {
        self.previous_desire.fill(0.0);
        self.reset_next = true;
    }
    pub fn reset_next(&self) -> bool {
        self.reset_next
    }
    pub fn prepare(
        &mut self,
        warped: &[u8],
        mut desire: [f32; 8],
        traffic: [f32; 2],
        action: [f32; 2],
        prepare_only: bool,
    ) -> Result<Option<[f32; 12]>, Error> {
        if prepare_only {
            self.reset();
            return Ok(None);
        }
        if warped.len() != contract::WARPED_BYTES {
            return Err(Error::Contract("warped camera shape"));
        }
        desire[0] = 0.0;
        let mut packed = [0.0; 12];
        for i in 0..8 {
            packed[i] = if desire[i] - self.previous_desire[i] > 0.99 {
                desire[i]
            } else {
                0.0
            };
        }
        self.previous_desire = desire;
        packed[8..10].copy_from_slice(&traffic);
        packed[10..12].copy_from_slice(&action);
        contract::check_input(warped, &packed)?;
        Ok(Some(packed))
    }
    pub fn parse(
        &mut self,
        output: &[f32],
        deadline: Deadline,
    ) -> Result<DrivingPrediction, Error> {
        deadline.remaining()?;
        contract::check_output(output)?;
        self.reset_next = false;
        let parsed = DrivingPrediction::parse(&RawOutputs::new(output, &self.slices)?)?;
        deadline.remaining()?;
        Ok(parsed)
    }
}
