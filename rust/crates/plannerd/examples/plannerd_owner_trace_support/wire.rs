//! Real writer -> dynamic schema snapshot, with explicit fixture I/O metadata.
use capnp::{dynamic_value::Reader as Dynamic, message::ReaderOptions, serialize};
use openpilot_cereal::log_capnp::{event, longitudinal_plan::PlanningTrigger};
use openpilot_plannerd::{
    carrot::CarrotPlanner, fast_radar::FastReason, longitudinal_planner::LongitudinalPlanner,
    publication,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};
#[derive(Deserialize)]
pub struct Metadata {
    pub now_ns: u64,
    pub valid: bool,
    pub model_ns: u64,
    pub radar_ns: u64,
    pub live_ns: u64,
    pub fast_mask: u8,
    pub fast_id: i32,
    pub trigger: String,
    pub fast_reason: FastReason,
    pub lead_status: bool,
}
fn value(reader: Dynamic<'_>) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(match reader {
        Dynamic::Void => Value::Null,
        Dynamic::Bool(v) => json!(v),
        Dynamic::Int8(v) => json!(v),
        Dynamic::Int16(v) => json!(v),
        Dynamic::Int32(v) => json!(v),
        Dynamic::Int64(v) => json!(v),
        Dynamic::UInt8(v) => json!(v),
        Dynamic::UInt16(v) => json!(v),
        Dynamic::UInt32(v) => json!(v),
        Dynamic::UInt64(v) => json!(v),
        Dynamic::Float32(v) => json!(f64::from(v)),
        Dynamic::Float64(v) => json!(v),
        Dynamic::Enum(v) => json!(v
            .get_enumerant()?
            .ok_or("unknown enum")?
            .get_proto()
            .get_name()?
            .to_str()?),
        Dynamic::Text(v) => json!(v.to_str()?),
        Dynamic::List(v) => Value::Array(
            (0..v.len())
                .map(|i| value(v.get(i)?))
                .collect::<Result<_, _>>()?,
        ),
        Dynamic::Struct(v) => {
            let mut fields = Map::new();
            for field in v.get_schema().get_fields()? {
                let reader = v.get(field)?;
                if !v.has(field)? {
                    continue;
                }
                fields.insert(
                    field.get_proto().get_name()?.to_str()?.to_owned(),
                    value(reader)?,
                );
            }
            Value::Object(fields)
        }
        _ => return Err("unsupported plan schema value".into()),
    })
}
pub fn snapshot(
    owner: &mut LongitudinalPlanner,
    carrot: &CarrotPlanner,
    meta: &Metadata,
) -> Result<Value, Box<dyn std::error::Error>> {
    let trigger = match meta.trigger.as_str() {
        "modelV2" => PlanningTrigger::ModelV2,
        "liveTracks" => PlanningTrigger::LiveTracks,
        _ => return Err("invalid publication trigger fixture".into()),
    };
    // Execution timing is explicit zero in both references, never a performance
    // result. Restore the actual solver's time immediately after serialization.
    let saved = owner.mpc.times[0];
    owner.mpc.times[0] = 0.;
    let bytes = publication::longitudinal(
        owner,
        carrot,
        publication::Metadata {
            now_ns: meta.now_ns,
            valid: meta.valid,
            model_ns: meta.model_ns,
            radar_ns: meta.radar_ns,
            live_ns: meta.live_ns,
            planner_seconds: 0.,
            fast_seconds: 0.,
            fast_mask: meta.fast_mask,
            fast_id: meta.fast_id,
            trigger,
            fast_reason: meta.fast_reason,
            lead_status: meta.lead_status,
        },
    );
    owner.mpc.times[0] = saved;
    let bytes = bytes?;
    let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new())?;
    let event = message.get_root::<event::Reader<'_>>()?;
    let event::LongitudinalPlan(plan) = event.which()? else {
        return Err("wrong publication event".into());
    };
    Ok(
        json!({"logMonoTime":event.get_log_mono_time(),"valid":event.get_valid(),"longitudinalPlan":value(plan?.into())?}),
    )
}
