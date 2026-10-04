use super::{carlog::Carlog, fixture::Fixture, platform};
use crate::{
    batch::Ego,
    data::Data,
    databases::Databases,
    decoder::{hyundai::Environment, Config, Interface, Kind},
    numerics::Numerics,
    runtime::{Io, Metrics, Reason},
    settings::Native,
    wire, Error,
};
use openpilot_can::Packet;
use openpilot_logging::{
    log_site,
    producer::{Factory, Logger},
    record::{Level, Record},
    runtime::RuntimeDiagnostics,
    Fields, Number,
};
use openpilot_messaging::{runtime::PubMaster, services};
use openpilot_msgq::{MultiSubscriber, Subscription};
use openpilot_params::Params;
use rustix::time::ClockId;
use std::time::Duration;

pub struct NativeIo {
    pub subscriptions: MultiSubscriber,
    settings: Native,
    databases: Databases,
    numerics: Numerics,
    publisher: PubMaster,
    logger: Logger,
    carlog: Carlog,
    diagnostics: Option<RuntimeDiagnostics>,
    pub(super) fixture: Option<Fixture>,
}

impl NativeIo {
    pub fn new(databases: Databases, numerics: Numerics) -> Result<Self, Error> {
        let specifications = ["can", "carState"].map(|topic| {
            services::lookup(topic)
                .map(|service| Subscription {
                    endpoint: topic,
                    capacity: service.queue_size,
                    polled: true,
                })
                .ok_or(Error::Contract("radar IPC service absent"))
        });
        let [can, state] = specifications;
        let subscriptions = MultiSubscriber::queued_for_runtime(&[can?, state?])?;
        let publisher = PubMaster::for_runtime(&["liveTracks"])?;
        Ok(Self {
            subscriptions,
            settings: Native(Params::for_runtime()?),
            databases,
            numerics,
            publisher,
            logger: Factory::for_runtime()?.logger(),
            carlog: Carlog::new()?,
            diagnostics: None,
            fixture: None,
        })
    }
    pub fn car_params(&self) -> Result<Option<Vec<u8>>, Error> {
        let bytes = self.settings.raw("CarParams")?;
        Ok((!bytes.is_empty()).then_some(bytes))
    }
    pub fn drain(&mut self, index: usize) -> Result<Vec<Vec<u8>>, Error> {
        let mut output = Vec::new();
        while let Some(bytes) = self.subscriptions.receive_one(index)? {
            output.push(bytes);
        }
        Ok(output)
    }
    pub fn poll(&mut self) -> Result<(), Error> {
        self.subscriptions.poll_ready(Duration::from_millis(20))?;
        Ok(())
    }
    pub fn record(&mut self, metrics: Metrics, timing: [f64; 4]) -> Result<(), Error> {
        let diagnostics = self
            .diagnostics
            .as_mut()
            .ok_or(Error::Contract("radar diagnostics not initialized"))?;
        diagnostics.record(
            &mut self.logger,
            log_site!(),
            [
                ("work_ms".to_owned(), Number::Float(timing[0])),
                ("thread_cpu_ms".to_owned(), Number::Float(timing[1])),
                ("decode_ms".to_owned(), Number::Float(timing[2])),
                ("radar_ms".to_owned(), Number::Float(timing[3])),
                (
                    "input_age_ms".to_owned(),
                    Number::Float(metrics.input_age_ms),
                ),
                (
                    "processed_batches".to_owned(),
                    Number::Integer(
                        i64::try_from(metrics.processed_batches)
                            .map_err(|_| Error::IntegerOverflow)?,
                    ),
                ),
                (
                    "invalid".to_owned(),
                    Number::Integer(u8::from(metrics.invalid).into()),
                ),
                (
                    "pending_states".to_owned(),
                    Number::Integer(
                        i64::try_from(metrics.pending_states)
                            .map_err(|_| Error::IntegerOverflow)?,
                    ),
                ),
                (
                    "pending_can".to_owned(),
                    Number::Integer(
                        i64::try_from(metrics.pending_can).map_err(|_| Error::IntegerOverflow)?,
                    ),
                ),
            ],
            Fields::new(),
        )?;
        Ok(())
    }
    fn warnings(&self, state: &mut Interface) {
        fn drain(reader: Option<&mut crate::reader::Reader>, carlog: &Carlog) {
            if let Some(reader) = reader {
                for diagnostic in reader.parser.diagnostics.drain(..) {
                    carlog.warning(&diagnostic.message);
                }
            }
        }
        drain(state.reader.as_mut(), &self.carlog);
        if let Kind::Hyundai(hyundai) = &mut state.kind {
            for reader in [
                hyundai.rcp_tracks.as_mut(),
                hyundai.rcp_scc.as_mut(),
                hyundai.rcp_corner_objects.as_mut(),
                hyundai.rcp_corner_objects_180.as_mut(),
            ] {
                drain(reader, &self.carlog);
            }
        }
    }
}

impl Io for NativeIo {
    fn monotonic_ns(&mut self) -> u64 {
        platform::monotonic_ns()
    }
    fn create_interface(&mut self, config: &Config) -> Result<Interface, Error> {
        let start = self.fixture.as_ref().map(|_| platform::monotonic_ns());
        let result = Interface::with_settings(
            config.clone(),
            &mut Environment {
                databases: &mut self.databases,
                clock: &mut platform::monotonic_ns,
                emit: &mut |text: &str| print!("{text}"),
                settings: &mut self.settings,
            },
        );
        if let (Some(fixture), Some(start)) = (self.fixture.take(), start) {
            fixture.constructed(start, result.as_ref().err())?;
        }
        result
    }
    fn track_flip(&mut self) -> Result<bool, Error> {
        let flip = self.settings.raw("RadarTrackFlip")? == b"1";
        self.diagnostics = Some(RuntimeDiagnostics::new("radarcan", 1.));
        Ok(flip)
    }
    fn update(
        &mut self,
        state: &mut Interface,
        ego: Ego,
        packets: &[Packet],
    ) -> Result<Option<Data>, Error> {
        let result = state.update_carrot(
            ego.v_ego,
            ego.a_ego,
            ego.receive_ns as f64 * 1e-9,
            packets,
            &mut self.numerics,
            &mut |text| print!("{text}"),
        );
        self.warnings(state);
        result
    }
    fn publish(&mut self, data: Data, valid: bool) -> Result<(), Error> {
        let timestamp = (platform::seconds(ClockId::Monotonic) * 1e9) as u64;
        self.publisher
            .send("liveTracks", &wire::encode(&data, valid, timestamp)?)?;
        Ok(())
    }
    fn input_error(&mut self, reason: Reason) -> Result<(), Error> {
        self.logger.emit(
            log_site!(),
            Record::text(Level::Error, format!("radarcan input invalid: {reason}")),
        )?;
        Ok(())
    }
}
