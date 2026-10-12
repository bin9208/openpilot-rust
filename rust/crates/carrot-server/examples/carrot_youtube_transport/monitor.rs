use openpilot_carrot_server::{
    youtube_live::{rtmp::Client, sink::Sink},
    Value,
};
use std::{
    io::Write,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn run(
    client: Arc<Client>,
    sink: &mut Sink,
    input: &Value,
) -> Result<(Value, Option<String>), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(input.get("input").string()?)?;
    let force = input.get("force_wake").truth();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let started = Instant::now();
            let error = sink
                .write_all(&bytes)
                .and_then(|_| sink.flush())
                .err()
                .map(|error| error.to_string());
            (started.elapsed().as_secs_f64(), error)
        });
        std::thread::sleep(Duration::from_secs(2));
        let health = client.try_connected()?.map_or(Value::Null, Value::Bool);
        let progress = client.bytes_written();
        if force {
            client.wake_shutdown();
        }
        let (seconds, error) = worker.join().map_err(|_| "fixture writer panicked")?;
        Ok((
            Value::object([
                ("health_during_write", health),
                ("progress_during_write", Value::integer(progress)),
                ("force_wake", Value::Bool(force)),
                ("write_seconds", Value::Float(seconds)),
                ("worker_joined", Value::Bool(true)),
            ]),
            error,
        ))
    })
}
