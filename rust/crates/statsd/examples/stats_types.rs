use openpilot_cereal::log_capnp::event;
use openpilot_statsd::producer::{Delivery, StatLog};
use std::io;
fn acknowledge(delivery: Delivery) -> Result<(), Box<dyn std::error::Error>> {
    if delivery != Delivery::Sent {
        return Err("metric dropped".into());
    }
    let mut line = String::new();
    if io::stdin().read_line(&mut line)? == 0 {
        return Err("peer acknowledgement missing".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut producer = StatLog::new(std::env::args().nth(1).ok_or("endpoint")?);
    let mut packet = capnp::message::Builder::new_default();
    {
        let mut state = packet.init_root::<event::Builder<'_>>().init_device_state();
        state.set_gpu_usage_percent(6);
        state.set_memory_usage_percent(42);
        state.set_screen_brightness_percent(0);
        state.set_fan_speed_percent_desired(65535);
        state.set_free_space_percent(62.1);
        let mut cpu = state.reborrow().init_cpu_usage_percent(2);
        cpu.set(0, -128);
        cpu.set(1, 127);
        state.init_cpu_temp_c(1).set(0, 40.2);
    }
    let root = packet.get_root_as_reader::<event::Reader<'_>>()?;
    let event::DeviceState(state) = root.which()? else {
        return Err("deviceState expected".into());
    };
    let state = state?;
    acknowledge(producer.gauge("gpu_usage_percent", state.get_gpu_usage_percent())?)?;
    acknowledge(producer.gauge("memory_usage_percent", state.get_memory_usage_percent())?)?;
    acknowledge(producer.gauge(
        "fan_speed_percent_desired",
        state.get_fan_speed_percent_desired(),
    )?)?;
    acknowledge(producer.gauge(
        "screen_brightness_percent",
        state.get_screen_brightness_percent(),
    )?)?;
    acknowledge(producer.gauge("free_space_percent", state.get_free_space_percent())?)?;
    for (index, value) in state.get_cpu_usage_percent()?.iter().enumerate() {
        acknowledge(producer.gauge(&format!("cpu{index}_usage_percent"), value)?)?;
    }
    for (index, value) in state.get_cpu_temp_c()?.iter().enumerate() {
        acknowledge(producer.gauge(&format!("cpu{index}_temperature"), value)?)?;
    }
    acknowledge(producer.sample("integer_sample", 6_i8)?)?;
    acknowledge(producer.sample("float_sample", -0.0_f64)?)?;
    Ok(())
}
