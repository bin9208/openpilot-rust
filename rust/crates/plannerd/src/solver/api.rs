use super::Kind;
use crate::Error;
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void},
    path::Path,
};

type Pointer = *mut c_void;
pub(super) type CapsuleCall = unsafe extern "C" fn(Pointer) -> c_int;
pub(super) type ModelSet =
    unsafe extern "C" fn(Pointer, Pointer, Pointer, c_int, *const c_char, Pointer) -> c_int;
pub(super) type Output =
    unsafe extern "C" fn(Pointer, Pointer, Pointer, c_int, *const c_char, Pointer);
pub(super) type MatrixDimensions =
    unsafe extern "C" fn(Pointer, Pointer, Pointer, c_int, *const c_char, *mut c_int);

pub(super) struct Api {
    _library: Library,
    pub create_capsule: unsafe extern "C" fn() -> Pointer,
    pub create: CapsuleCall,
    pub free: CapsuleCall,
    pub free_capsule: CapsuleCall,
    pub solve: CapsuleCall,
    pub reset: unsafe extern "C" fn(Pointer, c_int) -> c_int,
    pub update_params: unsafe extern "C" fn(Pointer, c_int, *mut f64, c_int) -> c_int,
    pub get_config: unsafe extern "C" fn(Pointer) -> Pointer,
    pub get_dims: unsafe extern "C" fn(Pointer) -> Pointer,
    pub get_input: unsafe extern "C" fn(Pointer) -> Pointer,
    pub get_output: unsafe extern "C" fn(Pointer) -> Pointer,
    pub get_solver: unsafe extern "C" fn(Pointer) -> Pointer,
    pub cost_set: ModelSet,
    pub constraints_set: ModelSet,
    pub out_set: Output,
    pub out_get: Output,
    pub dimensions: unsafe extern "C" fn(Pointer, Pointer, Pointer, c_int, *const c_char) -> c_int,
    pub cost_dimensions: MatrixDimensions,
    pub constraint_dimensions: MatrixDimensions,
    pub evaluate_cost: unsafe extern "C" fn(Pointer, Pointer, Pointer),
    pub get: unsafe extern "C" fn(Pointer, Pointer, *const c_char, Pointer),
}

impl Api {
    pub fn load(directory: &Path, kind: Kind) -> Result<Self, Error> {
        let prefix = kind.prefix();
        // SAFETY: artifact::verify binds every native library to the trusted build manifest.
        let library =
            unsafe { Library::new(directory.join(format!("libacados_ocp_solver_{prefix}.so")))? };
        macro_rules! symbol {
            ($name:expr) => {{
                // SAFETY: each inferred function type matches the pinned generated/acados header.
                unsafe { *library.get($name.as_bytes())? }
            }};
        }
        Ok(Self {
            create_capsule: symbol!(format!("{prefix}_acados_create_capsule")),
            create: symbol!(format!("{prefix}_acados_create")),
            free: symbol!(format!("{prefix}_acados_free")),
            free_capsule: symbol!(format!("{prefix}_acados_free_capsule")),
            solve: symbol!(format!("{prefix}_acados_solve")),
            reset: symbol!(format!("{prefix}_acados_reset")),
            update_params: symbol!(format!("{prefix}_acados_update_params")),
            get_config: symbol!(format!("{prefix}_acados_get_nlp_config")),
            get_dims: symbol!(format!("{prefix}_acados_get_nlp_dims")),
            get_input: symbol!(format!("{prefix}_acados_get_nlp_in")),
            get_output: symbol!(format!("{prefix}_acados_get_nlp_out")),
            get_solver: symbol!(format!("{prefix}_acados_get_nlp_solver")),
            cost_set: symbol!("ocp_nlp_cost_model_set"),
            constraints_set: symbol!("ocp_nlp_constraints_model_set"),
            out_set: symbol!("ocp_nlp_out_set"),
            out_get: symbol!("ocp_nlp_out_get"),
            dimensions: symbol!("ocp_nlp_dims_get_from_attr"),
            cost_dimensions: symbol!("ocp_nlp_cost_dims_get_from_attr"),
            constraint_dimensions: symbol!("ocp_nlp_constraint_dims_get_from_attr"),
            evaluate_cost: symbol!("ocp_nlp_eval_cost"),
            get: symbol!("ocp_nlp_get"),
            _library: library,
        })
    }
}
