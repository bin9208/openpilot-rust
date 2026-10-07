use openpilot_camera_kernel::{
    CallResult, Device, DeviceHandle, DeviceOperation, MemoryPool, Session,
};
use openpilot_camerad::{
    packet::{PacketError, COMMAND_SIZE, PACKET_SIZE},
    sensor::{Register, SensorKind},
    sensor_packets,
};

#[derive(Debug, thiserror::Error)]
pub enum SensorError {
    #[error(transparent)]
    Kernel(#[from] openpilot_camera_kernel::Error),
    #[error(transparent)]
    Packet(#[from] PacketError),
    #[error("camera port {0} is outside 0..3")]
    Port(usize),
    #[error("camera sensor has no acquired session and device")]
    NotAcquired,
}

pub struct SensorPort<'pool, 'device> {
    request: &'device Device,
    pool: &'pool MemoryPool<'device>,
    sensor: Device,
    port: usize,
    kind: SensorKind,
    enabled: bool,
    session: Option<Session>,
    handle: Option<DeviceHandle>,
}

impl<'pool, 'device> SensorPort<'pool, 'device> {
    pub fn probe(
        request: &'device Device,
        pool: &'pool MemoryPool<'device>,
        sensor: Device,
        port: usize,
        enabled: bool,
    ) -> Result<Self, SensorError> {
        if port >= 3 {
            return Err(SensorError::Port(port));
        }
        camera_log!(Debug, "opened sensor for {port}");
        camera_log!(Debug, "-- Probing sensor {port}");
        let mut output = Self {
            request,
            pool,
            sensor,
            port,
            kind: SensorKind::Ar0231,
            enabled,
            session: None,
            handle: None,
        };
        let mut found = false;
        for kind in [SensorKind::Ar0231, SensorKind::Os04c10, SensorKind::Ox03c10] {
            output.kind = kind;
            if output.probe_current()?.code == 0 {
                found = true;
                break;
            }
        }
        if !found {
            camera_log!(Error, "** sensor {port} FAILED bringup, disabling");
            output.enabled = false;
            return Ok(output);
        }
        let (result, session) = request.create_session();
        camera_log!(
            Debug,
            "get session: {} 0x{:X}",
            result.code,
            session.0 as u32
        );
        output.session = Some(session);
        camera_log!(Debug, "-- Accessing sensor");
        output.handle = Some(output.sensor.acquire_sensor(session)?);
        camera_log!(Debug, "acquire sensor dev");
        camera_log!(Info, "-- Configuring sensor");
        output.write_registers(output.kind.config().init_reg_array)?;
        Ok(output)
    }

    fn probe_current(&self) -> Result<CallResult, SensorError> {
        let mut packet = self.pool.lease(PACKET_SIZE + COMMAND_SIZE * 2)?;
        let mut info = self.pool.lease(24)?;
        let mut power = self.pool.lease(196)?;
        let data = sensor_packets::probe(
            self.kind,
            self.port,
            info.handle() as i32,
            power.handle() as i32,
        )?;
        info.write(0, &data.info)?;
        power.write(0, &data.power)?;
        packet.write(0, data.packet.bytes())?;
        let result = self.sensor.probe_sensor(packet.handle());
        camera_log!(Debug, "probing the sensor: {}", result.code);
        if result.code == 0 {
            camera_log!(Debug, "-- Probing sensor {} success", self.port);
        }
        Ok(result)
    }

    pub fn kind(&self) -> SensorKind {
        self.kind
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn acquired(&self) -> Option<(Session, DeviceHandle)> {
        self.session.zip(self.handle)
    }

    pub fn start(&mut self) -> Result<(), SensorError> {
        if self.enabled {
            camera_log!(Debug, "starting sensor {}", self.port);
            self.write_registers(self.kind.config().start_reg_array)?;
        }
        Ok(())
    }

    pub fn poke(&mut self, request: i32) -> Result<(), SensorError> {
        let (session, device) = self.acquired().ok_or(SensorError::NotAcquired)?;
        let mut packet = self.pool.lease(PACKET_SIZE)?;
        packet.write(0, sensor_packets::poke(request)?.bytes())?;
        if self.sensor.configure(session, device, packet.handle()).code != 0 {
            camera_log!(Error, "** sensor {} FAILED poke, disabling", self.port);
            self.enabled = false;
        }
        Ok(())
    }

    pub fn write_registers(&mut self, registers: &[Register]) -> Result<(), SensorError> {
        let (session, device) = self.acquired().ok_or(SensorError::NotAcquired)?;
        let mut packet = self.pool.lease(PACKET_SIZE + COMMAND_SIZE)?;
        let length = registers
            .len()
            .checked_mul(8)
            .and_then(|length| length.checked_add(8))
            .ok_or(PacketError::Size)?;
        let mut payload = self.pool.lease(length)?;
        let (data, registers) = sensor_packets::i2c(
            registers,
            4,
            self.kind.config().data_word,
            payload.handle() as i32,
        )?;
        payload.write(0, &registers)?;
        packet.write(0, data.bytes())?;
        if self.sensor.configure(session, device, packet.handle()).code != 0 {
            camera_log!(Error, "** sensor {} FAILED i2c, disabling", self.port);
            self.enabled = false;
        }
        Ok(())
    }

    pub fn shutdown(&mut self) {
        if let Some(session) = self.session.take() {
            if let Some(handle) = self.handle.take() {
                let result = self
                    .sensor
                    .control_device(DeviceOperation::Release, session, handle);
                camera_log!(Debug, "release sensor: {}", result.code);
            }
            let result = self.request.destroy_session(session);
            camera_log!(Debug, "destroyed session {}: {}", self.port, result.code);
        }
    }
}

impl Drop for SensorPort<'_, '_> {
    fn drop(&mut self) {
        self.shutdown();
    }
}
