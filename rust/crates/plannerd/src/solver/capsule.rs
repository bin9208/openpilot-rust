use super::{api::Api, artifact, Kind, Statistic};
use crate::Error;
use std::{
    ffi::{c_void, CStr},
    path::Path,
    ptr::NonNull,
};

pub struct Acados {
    pub(super) kind: Kind,
    pub(super) native: OwnedCapsule,
    pub(super) config: NonNull<c_void>,
    pub(super) dims: NonNull<c_void>,
    pub(super) input: NonNull<c_void>,
    pub(super) output: NonNull<c_void>,
    solver: NonNull<c_void>,
}

pub(super) struct OwnedCapsule {
    pub api: Api,
    capsule: NonNull<c_void>,
    initialized: bool,
}

impl Drop for OwnedCapsule {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: successful create initialized this exclusively owned capsule.
            unsafe { (self.api.free)(self.capsule.as_ptr()) };
        }
        // SAFETY: create_capsule allocated this pointer; this is its sole owning guard.
        unsafe { (self.api.free_capsule)(self.capsule.as_ptr()) };
    }
}

impl Acados {
    pub fn load(directory: &Path, kind: Kind) -> Result<Self, Error> {
        artifact::verify(directory)?;
        let api = Api::load(directory, kind)?;
        // SAFETY: allocator signature matches the generated solver header.
        let capsule = NonNull::new(unsafe { (api.create_capsule)() })
            .ok_or(Error::Contract("acados capsule allocation failed"))?;
        let mut creating = OwnedCapsule {
            api,
            capsule,
            initialized: false,
        };
        // SAFETY: capsule is fresh, allocated by the same generated library.
        status("create", unsafe { (creating.api.create)(capsule.as_ptr()) })?;
        creating.initialized = true;
        macro_rules! pointer {
            ($getter:ident) => {{
                // SAFETY: initialized capsule getters return borrowed native objects.
                NonNull::new(unsafe { (creating.api.$getter)(capsule.as_ptr()) })
                    .ok_or(Error::Contract("acados returned a null object"))?
            }};
        }
        let config = pointer!(get_config);
        let dims = pointer!(get_dims);
        let input = pointer!(get_input);
        let output = pointer!(get_output);
        let solver = pointer!(get_solver);
        Ok(Self {
            kind,
            native: creating,
            config,
            dims,
            input,
            output,
            solver,
        })
    }

    pub const fn kind(&self) -> Kind {
        self.kind
    }

    pub fn solve(&mut self) -> i32 {
        // SAFETY: mutable ownership serializes calls; library and capsule remain alive.
        unsafe { (self.native.api.solve)(self.native.capsule.as_ptr()) }
    }

    pub fn reset(&mut self) -> Result<(), Error> {
        // SAFETY: initialized capsule and source reset_qp_solver_mem=1 contract.
        status("reset", unsafe {
            (self.native.api.reset)(self.native.capsule.as_ptr(), 1)
        })
    }

    pub fn cost(&mut self) -> f64 {
        // SAFETY: all three objects belong to this live, exclusively borrowed capsule.
        unsafe {
            (self.native.api.evaluate_cost)(
                self.solver.as_ptr(),
                self.input.as_ptr(),
                self.output.as_ptr(),
            )
        };
        self.double(c"cost_value")
    }

    pub fn statistic(&mut self, field: Statistic) -> f64 {
        self.double(field.name())
    }

    fn double(&mut self, field: &CStr) -> f64 {
        let mut value = 0.;
        // SAFETY: private callers select only native scalar-double fields; output has one f64.
        unsafe {
            (self.native.api.get)(
                self.config.as_ptr(),
                self.solver.as_ptr(),
                field.as_ptr(),
                std::ptr::from_mut(&mut value).cast(),
            )
        };
        value
    }

    pub(super) fn parameters(&mut self, stage: i32, values: &mut [f64]) -> Result<(), Error> {
        let count =
            i32::try_from(values.len()).map_err(|_| Error::Contract("solver parameter count"))?;
        // SAFETY: access::set checks stage and parameter count before this synchronous copying API.
        status("parameters", unsafe {
            (self.native.api.update_params)(
                self.native.capsule.as_ptr(),
                stage,
                values.as_mut_ptr(),
                count,
            )
        })
    }
}

pub(super) fn status(operation: &'static str, status: i32) -> Result<(), Error> {
    if status == 0 {
        Ok(())
    } else {
        Err(Error::Solver { operation, status })
    }
}
