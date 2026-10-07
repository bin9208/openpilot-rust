use super::{capsule::status, Acados, Field};
use crate::Error;

impl Acados {
    pub fn set(&mut self, stage: usize, field: Field, values: &[f64]) -> Result<(), Error> {
        let stage = self.checked(stage, field, values.len())?;
        let mut copy = [0.; 36];
        let buffer = copy
            .get_mut(..values.len())
            .ok_or(Error::Contract("solver input capacity"))?;
        buffer.copy_from_slice(values);
        self.set_buffer(stage, field, buffer)
    }

    fn set_buffer(&mut self, stage: i32, field: Field, values: &mut [f64]) -> Result<(), Error> {
        let api = &self.native.api;
        let name = field.name().as_ptr();
        let pointer = values.as_mut_ptr().cast();
        match field {
            Field::Parameters => self.parameters(stage, values),
            Field::State | Field::Control => {
                // SAFETY: checked field/stage dimensions match this distinct mutable stack buffer.
                unsafe {
                    (api.out_set)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.output.as_ptr(),
                        stage,
                        name,
                        pointer,
                    )
                };
                Ok(())
            }
            Field::Reference | Field::Weights | Field::LowerSlack => {
                // SAFETY: source cost dimensions checked; native API copies, never retains this buffer.
                status("cost_set", unsafe {
                    (api.cost_set)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.input.as_ptr(),
                        stage,
                        name,
                        pointer,
                    )
                })
            }
            Field::LowerBound | Field::UpperBound => {
                // SAFETY: only stage-zero bounds with the checked native state dimension are admitted.
                status("constraints_set", unsafe {
                    (api.constraints_set)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.input.as_ptr(),
                        stage,
                        name,
                        pointer,
                    )
                })
            }
        }
    }

    pub fn get(&mut self, stage: usize, field: Field, values: &mut [f64]) -> Result<(), Error> {
        match field {
            Field::State | Field::Control => {}
            Field::Parameters
            | Field::Reference
            | Field::Weights
            | Field::LowerSlack
            | Field::LowerBound
            | Field::UpperBound => return Err(Error::Contract("solver field is not an output")),
        }
        let stage = self.checked(stage, field, values.len())?;
        // SAFETY: checked native dimensions exactly match the caller's initialized output slice.
        unsafe {
            (self.native.api.out_get)(
                self.config.as_ptr(),
                self.dims.as_ptr(),
                self.output.as_ptr(),
                stage,
                field.name().as_ptr(),
                values.as_mut_ptr().cast(),
            )
        };
        Ok(())
    }

    fn checked(&mut self, stage: usize, field: Field, size: usize) -> Result<i32, Error> {
        let expected = field.shape(self.kind, stage)?;
        if size != expected[0] * expected[1].max(1) {
            return Err(Error::Contract("solver buffer dimensions"));
        }
        let stage = i32::try_from(stage).map_err(|_| Error::Contract("solver stage conversion"))?;
        let api = &self.native.api;
        let name = field.name().as_ptr();
        let mut dimensions = [0_i32; 2];
        match field {
            Field::Parameters => return Ok(stage),
            Field::State | Field::Control => {
                // SAFETY: allowed field and stage belong to this capsule; native query returns one int.
                dimensions[0] = unsafe {
                    (api.dimensions)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.output.as_ptr(),
                        stage,
                        name,
                    )
                };
            }
            Field::Reference | Field::Weights | Field::LowerSlack => {
                // SAFETY: native cost query writes exactly two ints for these supported fields.
                unsafe {
                    (api.cost_dimensions)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.output.as_ptr(),
                        stage,
                        name,
                        dimensions.as_mut_ptr(),
                    )
                };
            }
            Field::LowerBound | Field::UpperBound => {
                // SAFETY: native bound query writes exactly two ints for stage-zero bounds.
                unsafe {
                    (api.constraint_dimensions)(
                        self.config.as_ptr(),
                        self.dims.as_ptr(),
                        self.output.as_ptr(),
                        stage,
                        name,
                        dimensions.as_mut_ptr(),
                    )
                };
            }
        }
        if dimensions
            != [
                i32::try_from(expected[0]).map_err(|_| Error::Contract("solver rows"))?,
                i32::try_from(expected[1]).map_err(|_| Error::Contract("solver columns"))?,
            ]
        {
            return Err(Error::Contract(
                "generated solver dimensions differ from pinned ABI",
            ));
        }
        Ok(stage)
    }
}
