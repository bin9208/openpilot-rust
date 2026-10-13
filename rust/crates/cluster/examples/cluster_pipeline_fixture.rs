use capnp::{message::Builder, serialize};
use openpilot_cereal::log_capnp::event;
use openpilot_cluster::{
    h264::{Config, Control, Software},
    live::Input,
    renderer::{
        CaptureOptions, ClusterUiRenderer, FrameClock, Options, StatsProvider, SystemStats,
    },
    usb::{Display, ParamsRuntime},
};
use serde::Deserialize;
use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Request {
    mode: String,
    output: PathBuf,
    renderer_width: i32,
    renderer_height: i32,
    width: u32,
    height: u32,
    frames: u32,
    fps: u32,
    wait_ack: bool,
    soft_ack: bool,
}
struct FixedStats;
impl StatsProvider for FixedStats {
    fn sample(&mut self, _now: f64) -> SystemStats {
        SystemStats::default()
    }
    fn close(&mut self) {}
}
fn display(root: &Path, fps: u32) -> io::Result<Display<ParamsRuntime>> {
    let lock = root.join("bus.lock");
    let options = openpilot_cluster::usb::Options {
        display_fps: i32::try_from(fps).map_err(io::Error::other)?,
        ..Default::default()
    };
    let mut display = Display::new(
        options,
        ParamsRuntime::at(&root.join("params"), "fixture"),
        &lock,
    )
    .map_err(io::Error::other)?;
    display.set_orientation(2, false);
    display.open().map_err(io::Error::other)?;
    Ok(display)
}
fn publish(
    car: &mut openpilot_msgq::Publisher,
    device: &mut openpilot_msgq::Publisher,
    frame: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    event.set_log_mono_time(10_000_000_000 + u64::from(frame) * 100_000_000);
    let mut state = event.init_car_state();
    state.set_v_ego(5.0 + f32::from(u16::try_from(frame)?));
    state.set_a_ego(0.1);
    state.set_steering_angle_deg(3.0);
    car.send(&serialize::write_message_to_words(&message))?;
    let mut message = Builder::new_default();
    let mut event = message.init_root::<event::Builder<'_>>();
    event.set_valid(true);
    event.set_log_mono_time(10_000_000_000);
    event.init_device_state().set_started(true);
    device.send(&serialize::write_message_to_words(&message))?;
    Ok(())
}
fn state(
    input: &mut Input,
    frame: u32,
) -> Result<openpilot_cluster::models::ClusterUiState, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        input.poll()?;
        let value = input.project_at(10.0 + f64::from(frame) / 10.0)?;
        if (value.speed_kph - (5.0 + f64::from(frame)) * 3.6).abs() < 1e-5 && value.onroad {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err("owned live Cereal frame did not project".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, request] = args.as_slice() else {
        return Err("cluster_pipeline_fixture ROOT REQUEST".into());
    };
    let request: Request = serde_json::from_slice(&std::fs::read(request)?)?;
    std::fs::create_dir_all(&request.output)?;
    let params = openpilot_params::Params::open(&request.output.join("params"), "fixture")?;
    params.put_bool("UsbGpuActive", request.fps == 5)?;
    let capacity = |name| {
        openpilot_messaging::services::lookup(name)
            .map(|service| service.queue_size)
            .ok_or("service missing")
    };
    let mut car = openpilot_msgq::Publisher::with_capacity("carState", capacity("carState")?)?;
    let mut device =
        openpilot_msgq::Publisher::with_capacity("deviceState", capacity("deviceState")?)?;
    let mut input = Input::isolated(false, 0, 10.0)?;
    let mut renderer = ClusterUiRenderer::new(
        Path::new(root),
        Options {
            width: request.renderer_width,
            height: request.renderer_height,
            theme_mode: "light".into(),
            screen_mode: 1,
            ..Options::default()
        },
    );
    renderer.set_stats_provider(Box::new(FixedStats));
    let config = Config {
        width: request.width,
        height: request.height,
        fps: request.fps,
        bitrate: "800k".into(),
        gop: 10,
        ffmpeg: "/usr/bin/ffmpeg".into(),
        encoder: "libx264".into(),
        requested_chunk_size: 4096,
        wait_for_ack: request.wait_ack,
        soft_ack: request.soft_ack,
        dump: Some(request.output.join("dump.h264")),
    };
    let receipt = if request.mode == "software" {
        software(
            &request,
            &config,
            &mut input,
            &mut renderer,
            &mut car,
            &mut device,
        )?
    } else {
        hardware(
            &request,
            &config,
            &mut input,
            &mut renderer,
            &mut car,
            &mut device,
        )?
    };
    renderer.close()?;
    std::fs::write(
        request.output.join("result.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}
fn software(
    request: &Request,
    config: &Config,
    input: &mut Input,
    renderer: &mut ClusterUiRenderer,
    car: &mut openpilot_msgq::Publisher,
    device: &mut openpilot_msgq::Publisher,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let root = request.output.clone();
    let fps = request.fps;
    let mut pipeline = Software::start(config.clone(), move || display(&root, fps))?;
    let pid = pipeline.encoder_pid().ok_or("encoder child absent")?;
    let command = std::fs::read(format!("/proc/{pid}/cmdline"))?;
    let mut rgba = File::create(request.output.join("frames.rgba"))?;
    let mut states = Vec::new();
    pipeline.control(Control::Brightness(61))?;
    pipeline.control(Control::Orientation(1))?;
    for frame in 0..request.frames {
        publish(car, device, frame)?;
        let state = state(input, frame)?;
        let clock = FrameClock {
            monotonic: 10.0 + f64::from(frame) / 10.0,
            perf_counter: 10.0,
            wall_time: 1_700_000_000.0,
        };
        let image = renderer.capture_state_at(
            &state,
            clock,
            CaptureOptions {
                portrait_upload: true,
                output_width: Some(i32::try_from(request.width)?),
                output_height: Some(i32::try_from(request.height)?),
            },
        )?;
        assert_eq!(
            (image.width, image.height),
            (
                i32::try_from(request.width)?,
                i32::try_from(request.height)?
            )
        );
        if frame == 0 || frame + 1 == request.frames {
            renderer.write_png(&image, &request.output.join(format!("frame-{frame}.png")))?;
        }
        rgba.write_all(&image.rgba)?;
        pipeline.submit_rgba(&image.rgba, request.width, request.height)?;
        states.push(
            serde_json::json!({"frame":frame,"speed_kph":state.speed_kph,"onroad":state.onroad}),
        );
    }
    pipeline.close()?;
    pipeline.close()?;
    let snapshot = pipeline.snapshot()?;
    Ok(
        serde_json::json!({"mode":"software","states":states,"submitted":request.frames,"bytes_sent":snapshot.bytes_sent,"chunks_sent":snapshot.chunks_sent,"error":snapshot.error,"reaped":!Path::new(&format!("/proc/{pid}")).exists(),"command":String::from_utf8(command)?.split('\0').filter(|value|!value.is_empty()).collect::<Vec<_>>() }),
    )
}

#[cfg(feature = "native-media")]
fn hardware(
    request: &Request,
    config: &Config,
    input: &mut Input,
    renderer: &mut ClusterUiRenderer,
    car: &mut openpilot_msgq::Publisher,
    device: &mut openpilot_msgq::Publisher,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use openpilot_cluster::{
        h264::{vidc, Hardware},
        renderer::nv12::{Nv12Packer, Nv12Request},
    };
    let root = request.output.clone();
    let fps = request.fps;
    let mut pipeline = Hardware::start(
        vidc::Config {
            width: i32::try_from(request.width)?,
            height: i32::try_from(request.height)?,
            fps: i32::try_from(fps)?,
            bitrate: 800_000,
            gop: 10,
            ..vidc::Config::default()
        },
        config.clone(),
        move || display(&root, fps),
    )?;
    pipeline.control(Control::Brightness(61))?;
    pipeline.control(Control::Orientation(1))?;
    let cancelled = pipeline
        .render_input(false, |_, fd, _| {
            assert!(rustix::fs::fstat(fd).is_ok());
            Err(io::Error::other("owned render cancellation"))
        })
        .unwrap_err()
        .to_string();
    let mut packer = Nv12Packer::default();
    let mut states = Vec::new();
    for frame in 0..request.frames {
        publish(car, device, frame)?;
        let state = state(input, frame)?;
        let clock = FrameClock {
            monotonic: 10.0 + f64::from(frame) / 10.0,
            perf_counter: 10.0,
            wall_time: 1_700_000_000.0,
        };
        renderer.prepare_trip_report_cache(&state, clock.monotonic)?;
        let mut frame_bytes = None;
        pipeline.render_input(false, |bytes, fd, layout| {
            assert!(rustix::fs::fstat(fd).is_ok());
            let output = renderer
                .render_to_nv12_buffer(
                    &mut packer,
                    Nv12Request {
                        layout,
                        buffer: Vec::new(),
                        flip_x: false,
                    },
                    |renderer| renderer.render_at(&state, clock, None),
                )
                .map_err(io::Error::other)?;
            bytes.copy_from_slice(&output);
            frame_bytes = Some(output);
            Ok(())
        })?;
        std::fs::write(
            request.output.join(format!("frame-{frame}.nv12")),
            frame_bytes.ok_or("GPU pack absent")?,
        )?;
        states.push(
            serde_json::json!({"frame":frame,"speed_kph":state.speed_kph,"onroad":state.onroad}),
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let layout = pipeline.input_layout();
    pipeline.close()?;
    pipeline.close()?;
    packer.close();
    let snapshot = pipeline.snapshot()?;
    Ok(
        serde_json::json!({"mode":"hardware","layout":layout,"states":states,"submitted":request.frames,"cancelled":cancelled,"bytes_sent":snapshot.bytes_sent,"chunks_sent":snapshot.chunks_sent,"dropped_units":snapshot.dropped_units,"dropped_chunks":snapshot.dropped_chunks,"error":snapshot.error}),
    )
}
#[cfg(not(feature = "native-media"))]
fn hardware(
    _: &Request,
    _: &Config,
    _: &mut Input,
    _: &mut ClusterUiRenderer,
    _: &mut openpilot_msgq::Publisher,
    _: &mut openpilot_msgq::Publisher,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Err("hardware requires native-media".into())
}
