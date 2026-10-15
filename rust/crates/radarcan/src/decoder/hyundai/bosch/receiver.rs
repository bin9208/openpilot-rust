//! Bosch-local receive context, held observations and final alias/acceleration writes.
use super::{
    acceleration,
    context::{Cache, Message, Pending, Pose},
    model_snapshot::Input,
    physical::Object,
    points,
    provider::{self, Provider},
};
use crate::{data::Data, Error};
use openpilot_can::Packet;
use std::collections::{BTreeMap, BTreeSet};
pub struct Receiver {
    pub provider: Provider,
    pub cache: Cache,
    pub pending: Option<Pending>,
    pub objects: Vec<Object>,
    pub now: i64,
    pub lead_acceleration: acceleration::Estimator,
}
impl Receiver {
    pub fn new(config: provider::Config) -> Result<Self, String> {
        Ok(Self {
            provider: Provider::new(config)?,
            cache: Cache::default(),
            pending: None,
            objects: vec![],
            now: 0,
            lead_acceleration: acceleration::Estimator::new(),
        })
    }
    pub fn set_context(&mut self, now: i64, pose: Message<Pose>, model: Message<Input>) {
        self.pending = Some(self.cache.set(&mut self.provider, now, pose, model));
    }
    pub fn update(
        &mut self,
        ego: f64,
        packets: &[Packet],
        fallback: impl FnOnce() -> i64,
    ) -> Result<bool, Error> {
        let pending = self.pending.take();
        let fallback_now = if pending.is_none() { fallback() } else { 0 };
        let (now, yaw, cues, path, path_ns, source_ns) =
            pending
                .as_ref()
                .map_or((fallback_now, None, &[][..], &[][..], None, None), |p| {
                    (
                        p.now,
                        p.yaw,
                        p.cues.as_slice(),
                        p.path.as_slice(),
                        Some(p.path_ns),
                        Some(p.path_source_ns),
                    )
                });
        self.now = now;
        self.provider.b5_birth_defer.ingest_speed(now, ego);
        let packets: Vec<_> = packets
            .iter()
            .map(|p| {
                Ok(provider::Packet {
                    timestamp_ns: i64::try_from(p.mono_time).map_err(|_| Error::IntegerOverflow)?,
                    messages: p
                        .frames
                        .iter()
                        .filter(|m| {
                            let bus = i32::from(m.bus);
                            let config = &self.provider.config;
                            (self.provider.camera.is_some()
                                && bus == config.camera_bus
                                && (0x738..=0x760).contains(&m.address))
                                || (bus == config.bus && (0x601..=0x612).contains(&m.address))
                                || (bus != config.bus
                                    && bus == config.scc_bus
                                    && matches!(m.address, 0x420 | 0x421))
                        })
                        .map(|m| provider::Message {
                            address: m.address,
                            payload: m.data.clone(),
                            bus: i32::from(m.bus),
                        })
                        .collect(),
                })
            })
            .collect::<Result<_, Error>>()?;
        let objects = self
            .provider
            .update(
                &packets,
                provider::Context {
                    now,
                    ego,
                    yaw,
                    vision: cues,
                    path,
                    path_ns,
                    path_source_ns: source_ns,
                },
            )
            .map_err(Error::BoschValue)?;
        if let Some(objects) = objects {
            self.objects = objects;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn append(&mut self, data: &mut Data, ego: f64) -> Result<(), Error> {
        let scan = self.provider.last_scan_timestamp_ns;
        let objects = if scan.is_some_and(|ns| (0..=150_000_000).contains(&(self.now - ns))) {
            self.objects.as_slice()
        } else {
            &[]
        };
        let live: BTreeSet<_> = self
            .provider
            .tracker
            .group_manager
            .states
            .keys()
            .copied()
            .collect();
        let aliases = self
            .provider
            .publication_aliases
            .update(
                self.now,
                &objects
                    .iter()
                    .map(|o| o.physical_track_id)
                    .collect::<Vec<_>>(),
                &live,
            )
            .map_err(Error::Contract)?;
        let aliases: BTreeMap<_, _> = aliases
            .into_iter()
            .map(|(pid, alias)| (pid, u64::from(alias)))
            .collect();
        let empty = BTreeMap::new();
        let accelerations = if self.provider.debug.timeout {
            self.lead_acceleration.reset("PROVIDER_TIMEOUT");
            &empty
        } else {
            self.lead_acceleration.update(objects, scan, ego, &live)?
        };
        let objects = self
            .provider
            .publication_view(objects, Some(self.now))
            .map_err(Error::BoschValue)?;
        points::append(
            data,
            &objects,
            ego,
            self.now,
            Some(&aliases),
            Some(accelerations),
        )
        .map_err(|e| Error::BoschValue(e.message))
    }
}
