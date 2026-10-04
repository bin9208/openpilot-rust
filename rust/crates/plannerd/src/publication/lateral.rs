use super::values;
use crate::{
    lane_departure::Warning, lateral_planner::LateralPlanner, model::ModelMeta, number::float32,
    Error,
};
use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;

pub fn lateral(
    owner: &LateralPlanner,
    metadata: &ModelMeta,
    now_ns: u64,
    model_ns: u64,
    valid: bool,
    debug: bool,
) -> Result<Vec<u8>, Error> {
    let output = owner.output();
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(now_ns);
    event.set_valid(valid);
    let mut plan = event.init_lateral_plan();
    plan.set_model_mono_time(model_ns);
    values(plan.reborrow().init_d_path_points(33), &output.path_points)?;
    values(plan.reborrow().init_psis(17), &output.headings)?;
    values(plan.reborrow().init_distances(17), &output.distances)?;
    values(plan.reborrow().init_curvatures(17), &output.curvatures)?;
    values(
        plan.reborrow().init_curvature_rates(17),
        &output.curvature_rates,
    )?;
    plan.set_mpc_solution_valid(output.solution_valid);
    plan.set_solver_execution_time(float32(owner.mpc.solve_time)?);
    if debug {
        plan.set_solver_cost(float32(owner.mpc.cost)?);
        let mut solver = plan.reborrow().init_solver_state();
        let mut states = solver.reborrow().init_x(33);
        for (index, row) in owner.mpc.x.iter().enumerate() {
            values(
                states.reborrow().init(
                    u32::try_from(index).map_err(|_| Error::Contract("lateral state index"))?,
                    4,
                ),
                row,
            )?;
        }
        values(solver.reborrow().init_u(32), &owner.mpc.u.map(|row| row[0]))?;
    }
    plan.set_use_lane_lines(output.use_lane_lines);
    plan.set_lane_change_state(metadata.lane_change_state);
    plan.set_lane_change_direction(metadata.lane_change_direction);
    plan.set_lane_width(float32(output.lane_width)?);
    let mut position = plan.reborrow().init_position();
    values(position.reborrow().init_x(33), &output.position[0])?;
    values(position.reborrow().init_y(33), &output.position[1])?;
    values(position.reborrow().init_z(33), &output.position[2])?;
    plan.set_lat_debug_text(&output.debug_text);
    Ok(serialize::write_message_to_words(&message))
}

pub fn assistance(warning: Warning, now_ns: u64, valid: bool) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_log_mono_time(now_ns);
    event.set_valid(valid);
    let mut assistance = event.init_driver_assistance();
    assistance.set_left_lane_departure(warning.left);
    assistance.set_right_lane_departure(warning.right);
    serialize::write_message_to_words(&message)
}
