//! Raw CAN replay uses production batch joins, including radarInput metadata.
use openpilot_can::Packet;
use openpilot_radarcan::{
    self as radarcan,
    batch::Ego,
    data::Data,
    databases::Databases,
    decoder::{
        hyundai::{
            bosch::{
                context::{Message, Pose},
                model_snapshot::Input,
            },
            Environment,
        },
        Config, Interface,
    },
    native::Carlog,
    numerics::Numerics,
    runtime::{Engine, Io, Reason},
    settings::Settings,
    Error,
};
use std::{collections::BTreeMap, path::Path};

pub struct Parameters(pub BTreeMap<String, i32>);
impl Settings for Parameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        self.0
            .get(key)
            .copied()
            .ok_or(Error::Contract("offline radar integer parameter missing"))
    }
}
struct Offline {
    now: u64,
    databases: Databases,
    numerics: Numerics,
    settings: Parameters,
    publications: Vec<Vec<u8>>,
    errors: Vec<String>,
    flipped: bool,
    carlog: Carlog,
    bosch_pose: Message<Pose>,
    bosch_model: Message<Input>,
    bosch_replay_ns: u64,
}
impl Io for Offline {
    fn monotonic_ns(&mut self) -> u64 {
        self.now
    }
    fn create_interface(&mut self, config: &Config) -> Result<Interface, Error> {
        let now = self.now;
        Interface::with_settings(
            config.clone(),
            &mut Environment {
                databases: &mut self.databases,
                clock: &mut || now,
                emit: &mut |text: &str| eprint!("{text}"),
                settings: &mut self.settings,
            },
        )
    }
    fn track_flip(&mut self) -> Result<bool, Error> {
        Ok(self.flipped)
    }
    fn update(
        &mut self,
        interface: &mut Interface,
        ego: Ego,
        packets: &[Packet],
    ) -> Result<Option<Data>, Error> {
        if interface.supports_bosch() {
            // Source replay time advances on matched nonempty CAN only. Scene
            // timestamps may be in the future and remain cached until causal.
            if !packets.is_empty() {
                self.bosch_replay_ns = self.bosch_replay_ns.max(ego.last_can_ns);
            }
            interface.set_bosch_context(
                i64::try_from(self.bosch_replay_ns).map_err(|_| Error::IntegerOverflow)?,
                self.bosch_pose.clone(),
                self.bosch_model.clone(),
            );
        }
        #[allow(clippy::cast_precision_loss)]
        let result = interface.update_carrot(
            ego.v_ego,
            ego.a_ego,
            ego.receive_ns as f64 * 1e-9,
            packets,
            &mut self.numerics,
            &mut |text| eprint!("{text}"),
        );
        // Warnings are source console effects, including failed update cycles;
        // they must not become synthetic input-error publications.
        for warning in interface.take_warnings() {
            self.carlog.warning(&warning);
        }
        result
    }
    fn publish(&mut self, data: Data, valid: bool) -> Result<(), Error> {
        self.publications
            .push(radarcan::wire::encode(&data, valid, self.now)?);
        Ok(())
    }
    fn input_error(&mut self, reason: Reason) -> Result<(), Error> {
        self.errors.push(reason.to_string());
        Ok(())
    }
}
pub struct Raw {
    engine: Engine,
    io: Offline,
}
impl Raw {
    pub fn new(
        car_params: &[u8],
        dbc: &Path,
        numerics: &Path,
        settings: BTreeMap<String, i32>,
        flipped: bool,
    ) -> Result<Self, Error> {
        let config = radarcan::wire::config(car_params)?;
        let mut io = Offline {
            now: 0,
            databases: Databases::new(dbc.to_path_buf()),
            numerics: Numerics::load(numerics)?,
            settings: Parameters(settings),
            publications: Vec::new(),
            errors: Vec::new(),
            flipped,
            carlog: Carlog::new()?,
            bosch_pose: Message::default(),
            bosch_model: Message::default(),
            bosch_replay_ns: 0,
        };
        let engine = Engine::new(config, &mut io, true)?;
        Ok(Self { engine, io })
    }
    /// Separate conflated scene streams do not consume or advance CAN joins.
    /// Context remains in IO across Engine interface rebuilds.
    pub fn scene(&mut self, bytes: &[u8], pose: bool) -> Result<(), Error> {
        if self.engine.radar.supports_bosch() {
            if pose {
                self.io.bosch_pose = radarcan::wire::bosch_pose(bytes)?;
            } else {
                self.io.bosch_model = radarcan::wire::bosch_model(bytes)?;
            }
        }
        Ok(())
    }
    pub fn feed(
        &mut self,
        bytes: &[u8],
        now: u64,
        can: bool,
    ) -> Result<(Vec<Vec<u8>>, Vec<String>), Error> {
        self.io.now = now;
        if can {
            self.engine.batches.add_can([radarcan::wire::can(bytes)?]);
        } else {
            self.engine.add_state(radarcan::wire::ego(bytes)?);
            self.engine.process(&mut self.io)?;
        }
        Ok((
            std::mem::take(&mut self.io.publications),
            std::mem::take(&mut self.io.errors),
        ))
    }
}
