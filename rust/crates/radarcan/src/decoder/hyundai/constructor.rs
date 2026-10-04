use super::{group3, identity, Hyundai};
use crate::{
    base::Base,
    databases::Databases,
    decoder::Config,
    integer_set::IntegerSet,
    point::{Point, Source},
    reader::Reader,
    settings::Settings,
    Error,
};

pub struct Environment<'a, C, E, S> {
    pub databases: &'a mut Databases,
    pub clock: &'a mut C,
    pub emit: &'a mut E,
    pub settings: &'a mut S,
}

impl<C: FnMut() -> u64, E: FnMut(&str), S: Settings> Environment<'_, C, E, S> {
    fn reader(
        &mut self,
        name: &str,
        first: u32,
        count: u32,
        required: u32,
        bus: i64,
        frequency: f64,
    ) -> Result<Reader, Error> {
        Reader::new(
            self.databases,
            name,
            (first..first + count).map(|address| {
                (
                    address,
                    if address < first + required {
                        frequency
                    } else {
                        f64::NAN
                    },
                )
            }),
            bus,
            self.clock,
            self.emit,
        )
    }

    fn buses(&mut self, config: &Config) -> Result<(i64, i64, i64), Error> {
        let offset = (i64::try_from(config.safety_count).map_err(|_| Error::IntegerOverflow)? - 1)
            .checked_mul(4)
            .ok_or(Error::IntegerOverflow)?;
        let (a, e) = if config.flags & 1 != 0 && self.settings.integer("HyundaiCameraSCC")? == 0 {
            (0, 1)
        } else {
            (1, 0)
        };
        Ok((a + offset, e + offset, 2 + offset))
    }

    fn corner(
        &mut self,
        config: &Config,
        enabled: bool,
        name: &str,
        first: u32,
        count: u32,
        label: &str,
    ) -> Result<Option<Reader>, Error> {
        if !enabled || config.flags & 8192 == 0 {
            return Ok(None);
        }
        if !self.databases.exists(name) {
            (self.emit)(&format!(
                "RadarInterface: missing {name}.dbc, {label} corner radar disabled\n"
            ));
            return Ok(None);
        }
        let (bus, _, _) = self.buses(config)?;
        Ok(Some(self.reader(name, first, count, count, bus, 33.)?))
    }
}

impl Hyundai {
    pub fn new<C: FnMut() -> u64, E: FnMut(&str), S: Settings>(
        config: &Config,
        base: &mut Base,
        pt: Option<&str>,
        environment: &mut Environment<'_, C, E, S>,
    ) -> Result<Self, Error> {
        let canfd = config.flags & 8192 != 0;
        let group4 = !canfd && config.ext_flags & (1 << 15) != 0;
        let group1 = canfd && config.ext_flags & (1 << 7) != 0;
        let group3_enabled = canfd && !group1 && config.ext_flags & (1 << 11) != 0;
        let (first, count) = if group1 {
            (0x210, 16)
        } else if group3_enabled {
            (0x400, 30)
        } else if canfd {
            (0x3a5, 32)
        } else if group4 {
            (0x500, 8)
        } else {
            (0x500, 64)
        };
        let required = if !canfd && !group4 { 32 } else { count };
        let tracks = environment.settings.integer("EnableRadarTracks")? >= 1;
        let corner235 = config.ext_flags & (1 << 12) != 0
            && environment.settings.integer("EnableCornerRadar")? > 0;
        let corner180 = config.ext_flags & (1 << 13) != 0
            && environment.settings.integer("EnableCornerRadar")? > 0;
        let rcp_tracks = if tracks {
            (environment.emit)("RadarInterface: RadarTracks...\n");
            let (name, bus) = if canfd {
                let (bus, _, _) = environment.buses(config)?;
                ("hyundai_canfd_radar_generated", bus)
            } else if group4 {
                ("hyundai_kia_denso_front_radar_generated", 1)
            } else {
                ("hyundai_kia_mando_front_radar_generated", 1)
            };
            Some(environment.reader(name, first, count, required, bus, 20.)?)
        } else {
            None
        };
        let rcp_corner_objects = environment.corner(
            config,
            corner235,
            "hyundai_canfd_corner_radar_235_generated",
            0x235,
            20,
            "0x235",
        )?;
        let rcp_corner_objects_180 = environment.corner(
            config,
            corner180,
            "hyundai_canfd_corner_radar_180_generated",
            0x180,
            5,
            "0x180",
        )?;
        let rcp_scc = if tracks && !canfd && config.flags & 8 == 0 {
            None
        } else {
            let (_, e, cam) = environment.buses(config)?;
            let address = if canfd { 416 } else { 0x420 };
            (environment.emit)(&format!("$$$$$$$$ ECAN =  {e}\n"));
            let bus = if config.flags & 8 != 0 { cam } else { e };
            Some(environment.reader(
                pt.ok_or(Error::Contract("Hyundai powertrain DBC metadata absent"))?,
                address,
                1,
                1,
                bus,
                50.,
            )?)
        };
        let corners_available = rcp_corner_objects.is_some() || rcp_corner_objects_180.is_some();
        let radar_off_can = config.unavailable && !corners_available;
        (environment.emit)(&format!("RadarInterface: radarUnavailable={} radarTracks={} group4={} corner235={} corner180={} corner430=False radarOffCan={}\n",
            python_bool(config.unavailable),python_bool(tracks),python_bool(group4),python_bool(rcp_corner_objects.is_some()),
            python_bool(rcp_corner_objects_180.is_some()),python_bool(radar_off_can)));
        if rcp_tracks.is_some() {
            let total = count * if group1 { 2 } else { 1 };
            for slot in 32..32 + total {
                base.pts.insert(
                    u64::from(slot),
                    Point {
                        track_id: u64::from(slot),
                        ..Point::default()
                    },
                );
            }
        }
        if rcp_scc.is_some() {
            base.pts.insert(
                0,
                Point {
                    radar_source: Source::Scc,
                    ..Point::default()
                },
            );
        }
        for (enabled, first, count, source) in [
            (rcp_corner_objects.is_some(), 200, 20, Source::Corner235),
            (rcp_corner_objects_180.is_some(), 240, 10, Source::Corner180),
        ] {
            if enabled {
                for slot in first..first + count {
                    base.pts.insert(
                        slot,
                        Point {
                            track_id: slot,
                            radar_source: source,
                            ..Point::default()
                        },
                    );
                }
            }
        }
        Ok(Self {
            canfd,
            radar_group1: group1,
            radar_group3: group3_enabled,
            radar_group4: group4,
            radar_start_addr: first,
            radar_msg_count: count,
            radar_required_msg_count: required,
            radar_tracks: tracks,
            corner_object_tracks: corner235,
            corner_object_180_tracks: corner180,
            corner_object_430_tracks: false,
            rcp_tracks,
            rcp_scc,
            rcp_corner_objects,
            rcp_corner_objects_180,
            updated_tracks: IntegerSet::default(),
            updated_scc: IntegerSet::default(),
            updated_corner_objects: IntegerSet::default(),
            updated_corner_objects_180: IntegerSet::default(),
            updated_corner_objects_430: IntegerSet::default(),
            corner_object_missed_updates: 0,
            corner_object_180_missed_updates: 0,
            corner_object_430_missed_updates: 0,
            corner_object_track_ids: identity::TrackIds::default(),
            group3_track_ids: group3::TrackIds::default(),
            trigger_msg_tracks: first + required - 1,
            trigger_msg_scc: if canfd { 416 } else { 0x420 },
            trigger_msg_corner_objects: 0x248,
            trigger_msg_corner_objects_180: 0x184,
            trigger_msg_corner_objects_430: 0x447,
            corner_objects_available: corners_available,
            radar_off_can,
            track_id: 0,
            v_rel_last: 0.,
            d_rel_last: 0.,
        })
    }
}

fn python_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}
