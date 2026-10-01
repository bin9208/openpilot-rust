use crate::{
    image,
    state::{self, Shared, Stop},
    Error,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_messaging::{runtime::SubMaster, state::Options};
use openpilot_msgq::{VisionClient, VisionStream};
use openpilot_process_supervision::{
    Execution, ManagedProcess, NativeCommand, ProcessLog, StopOptions,
};
use serde_json::{json, Value};
use std::{fs, process::Command, time::Duration};

struct Owner<'a> {
    shared: &'a Shared,
    camera: ManagedProcess,
}
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.camera.stop(StopOptions::default()) {
            eprintln!("athenad: camera stop: {error}");
        }
        if let Err(error) = self.shared.params.put_bool("IsTakingSnapshot", false) {
            eprintln!("athenad: snapshot Params: {error}");
        }
        if let Err(error) = state::remove(&self.shared.params, "Offroad_IsTakingSnapshot") {
            eprintln!("athenad: snapshot alert: {error}");
        }
    }
}
pub fn take(shared: &Shared, stop: &Stop) -> Result<Value, Error> {
    if !state::boolean(&shared.params, "IsOffroad")?
        || state::boolean(&shared.params, "IsTakingSnapshot")?
    {
        return Ok(json!({"jpegBack":null,"jpegFront":null}));
    }
    let front = state::boolean(&shared.params, "RecordFront")?;
    shared.params.put_bool("IsTakingSnapshot", true)?;
    let launcher = shared.config.process_launcher.clone();
    let camera = ManagedProcess::new(
        "camerad".into(),
        Execution::Native(NativeCommand {
            launcher,
            basedir: shared.config.basedir.clone(),
            cwd: "openpilot/system/camerad".into(),
            argv: vec!["./camerad".into()],
        }),
        ProcessLog::new(shared.factory.logger()),
    );
    let mut owner = Owner { shared, camera };
    let alerts: Value = serde_json::from_slice(&fs::read(
        shared
            .config
            .basedir
            .join("openpilot/selfdrive/selfdrived/alerts_offroad.json"),
    )?)?;
    let mut alert = alerts["Offroad_IsTakingSnapshot"].clone();
    alert["extra"] = "".into();
    shared
        .params
        .put("Offroad_IsTakingSnapshot", &serde_json::to_vec(&alert)?)?;
    stop.wait(Duration::from_secs(2));
    if stop.requested() {
        return Err(Error::Stopped);
    }
    if Command::new("pgrep").arg("camerad").status()?.success() {
        return Ok(json!({"jpegBack":null,"jpegFront":null}));
    }
    if !shared.config.pc {
        owner.camera.start()?;
    }
    let (rear, driver) = capture(front, stop)?;
    Ok(
        json!({"jpegBack":STANDARD.encode(rear),"jpegFront":driver.map(|data|STANDARD.encode(data))}),
    )
}
pub fn capture(front: bool, stop: &Stop) -> Result<(Vec<u8>, Option<Vec<u8>>), Error> {
    let services = if front {
        vec!["wideRoadCameraState", "driverCameraState"]
    } else {
        vec!["wideRoadCameraState"]
    };
    let mut subscriber = SubMaster::for_runtime(&services, Options::default())?;
    let mut rear = VisionClient::new("camerad", VisionStream::WideRoad, true)?;
    let mut driver = front
        .then(|| VisionClient::new("camerad", VisionStream::Driver, true))
        .transpose()?;
    loop {
        if stop.requested() {
            return Err(Error::Stopped);
        }
        let topic = subscriber.state.topic("wideRoadCameraState")?;
        let openpilot_cereal::log_capnp::event::WideRoadCameraState(camera) =
            topic.event()?.which()?
        else {
            return Err(Error::Contract("wide camera event required"));
        };
        if camera?.get_frame_id() >= 80 {
            break;
        }
        subscriber.update(Duration::from_millis(100))?;
    }
    for client in std::iter::once(&mut rear).chain(driver.iter_mut()) {
        while !client.connect()? {
            if stop.requested() {
                return Err(Error::Stopped);
            }
            stop.wait(Duration::from_millis(100));
        }
    }
    let rear = photograph(&mut rear, stop)?;
    let driver = driver
        .as_mut()
        .map(|client| photograph(client, stop))
        .transpose()?;
    Ok((rear, driver))
}
fn photograph(client: &mut VisionClient, stop: &Stop) -> Result<Vec<u8>, Error> {
    if stop.requested() {
        return Err(Error::Stopped);
    }
    let frame = client
        .receive(Duration::from_millis(100))?
        .ok_or(Error::Contract("snapshot frame unavailable"))?;
    let mut buffer = vec![0; frame.metadata().len];
    frame.copy_into(&mut buffer)?;
    image::jpeg(
        &image::extract(&buffer, frame.metadata())?,
        frame.metadata().width,
        frame.metadata().height,
    )
}
