//! Carrot ARS510 receiver: independent control/display histories and input-fault recovery.
use super::{
    config::NativeInterfaceConfig,
    interface::{Assembler, NativeInterface, Output},
    transport::StrictId80RecordAssembler,
};
use crate::{base::Base, data::Data, numerics::Numerics, point::Point, Error};
use indexmap::IndexMap;
use openpilot_can::Packet;
use std::collections::BTreeMap;
const LIMIT: u64 = 1 << 31;
pub fn is_supported(brand: &str, candidate: &str, flags: u32) -> bool {
    openpilot_card::brands::toyota::supports_ars510(brand, candidate, flags)
}
const DECODER_KEYS: [&str; 5] = [
    "records",
    "crc_failures",
    "settling_suppressed",
    "unresolved_suppressed",
    "path_gated",
];
type Counters = BTreeMap<String, u64>;
fn inc(c: &mut Counters, key: &str) {
    *c.entry(key.into()).or_default() += 1;
}
fn stats(d: &NativeInterface) -> [u64; 5] {
    [
        d.records,
        d.crc_failures,
        d.settling_suppressed,
        d.unresolved_suppressed,
        d.path_gated,
    ]
}
fn strict(d: &NativeInterface) -> &StrictId80RecordAssembler {
    match &d._assembler {
        Assembler::Strict(a) => a,
        _ => unreachable!("Carrot decoder always uses strict assembly"),
    }
}
fn new_decoder(display: bool) -> NativeInterface {
    let mut config = NativeInterfaceConfig::fused();
    if display {
        config.path_gate_m = 0.;
    }
    let mut d = NativeInterface::new(config);
    d._assembler = Assembler::Strict(StrictId80RecordAssembler::default());
    d
}
fn empty(can_error: bool, unavailable: bool) -> Data {
    let mut d = Data {
        has_display_data: true,
        ..Data::default()
    };
    d.errors.can_error = can_error;
    d.errors.radar_unavailable_temporary = unavailable;
    d
}
#[derive(serde::Serialize)]
pub struct Receiver {
    pub decoder: NativeInterface,
    pub display_decoder: NativeInterface,
    _display_points: IndexMap<u64, Point>,
    _display_namespace_offset: u64,
    _display_max_track_id: u64,
    _display_counters: Counters,
    _display_decoder_counters: Counters,
    _counters: Counters,
    _decoder_counters: Counters,
    _assembler_counters: Counters,
    _record_time_s: Option<f64>,
    _packet_time_s: Option<f64>,
    _receive_time_s: Option<f64>,
    _completion_time_s: Option<f64>,
    _blocked_packet_time_s: Option<f64>,
    _namespace_offset: u64,
    _max_track_id: u64,
    _unavailable: bool,
    _seen_record: bool,
    _fault_latched: bool,
    _start_time_s: Option<f64>,
    _warned: bool,
    _outer_frame: u64,
    _inside_carrot_update: bool,
}
impl Default for Receiver {
    fn default() -> Self {
        Self {
            decoder: new_decoder(false),
            display_decoder: new_decoder(true),
            _display_points: IndexMap::new(),
            _display_namespace_offset: 1000,
            _display_max_track_id: 999,
            _display_counters: Counters::new(),
            _display_decoder_counters: Counters::new(),
            _counters: Counters::new(),
            _decoder_counters: Counters::new(),
            _assembler_counters: Counters::new(),
            _record_time_s: None,
            _packet_time_s: None,
            _receive_time_s: None,
            _completion_time_s: None,
            _blocked_packet_time_s: None,
            _namespace_offset: 1000,
            _max_track_id: 999,
            _unavailable: true,
            _seen_record: false,
            _fault_latched: false,
            _start_time_s: None,
            _warned: false,
            _outer_frame: 0,
            _inside_carrot_update: false,
        }
    }
}
impl Receiver {
    pub fn configure(base: &mut Base) {
        base.dt = Some(0.06);
        base.init_done = true;
    }
    pub fn recover_from_input_fault(&mut self, previous: &Self, base: &mut Base) {
        self._seen_record |= previous._seen_record;
        self._max_track_id = self
            ._max_track_id
            .max(self._namespace_offset - 1)
            .max(previous._max_track_id)
            .max(previous._namespace_offset - 1);
        self._display_max_track_id = self
            ._display_max_track_id
            .max(self._display_namespace_offset - 1)
            .max(previous._display_max_track_id)
            .max(previous._display_namespace_offset - 1);
        self._fault_latched = true;
        self.reset_history(base);
        inc(&mut self._counters, "input_recovery_resets");
    }
    fn reset_display(&mut self) {
        for (key, value) in DECODER_KEYS.into_iter().zip(stats(&self.display_decoder)) {
            *self
                ._display_decoder_counters
                .entry(key.into())
                .or_default() += value;
        }
        self.display_decoder = new_decoder(true);
        self._display_points.clear();
        self._display_namespace_offset = self._display_max_track_id + 1;
        inc(&mut self._display_counters, "history_resets");
    }
    fn display_error(&mut self, key: &str) {
        inc(&mut self._display_counters, key);
        self.reset_display();
    }
    fn reset_history(&mut self, base: &mut Base) {
        for (key, value) in DECODER_KEYS.into_iter().zip(stats(&self.decoder)) {
            *self._decoder_counters.entry(key.into()).or_default() += value;
        }
        for (key, value) in &strict(&self.decoder).counters {
            *self._assembler_counters.entry(key.clone()).or_default() += value;
        }
        self.decoder = new_decoder(false);
        self.reset_display();
        base.pts.clear();
        base.tracks.clear();
        self._namespace_offset = self._max_track_id + 1;
        self._record_time_s = None;
        self._completion_time_s = None;
        self._unavailable = true;
        inc(&mut self._counters, "history_resets");
    }
    fn input_error(&mut self, base: &mut Base) -> Data {
        self._fault_latched = true;
        self.reset_history(base);
        empty(true, true)
    }
    fn waiting(&mut self, time: Option<f64>, emit: &mut impl FnMut(&str)) -> Option<Data> {
        let now = time.filter(|_| self._unavailable)?;
        let start = *self._start_time_s.get_or_insert(now);
        if !self._seen_record && !self._fault_latched && !self._warned && now - start > 15. {
            emit("ARS510: 첫 CRC 정상 객체 레코드의 수신을 기다리고 있습니다");
            self._warned = true;
            inc(&mut self._counters, "cold_start_warnings");
        }
        if !self._outer_frame.is_multiple_of(5) {
            return None;
        }
        if self._seen_record || self._fault_latched {
            Some(empty(self._fault_latched, true))
        } else {
            inc(&mut self._counters, "cold_start_publications");
            Some(empty(false, false))
        }
    }
    fn points(
        decoded: Output,
        offset: u64,
        display: bool,
        counters: &mut Counters,
        maximum: &mut u64,
    ) -> Result<IndexMap<u64, Point>, &'static str> {
        let mut points = IndexMap::new();
        for p in decoded.radar_data.points {
            let Some(meta) = p.metadata else {
                return Err("decoder_exceptions");
            };
            if ![p.d_rel.0, p.y_rel.0, p.v_rel.0, meta.v_long_ground.0]
                .iter()
                .all(|v| v.is_finite())
                || p.d_rel.0 <= 0.
            {
                if display {
                    return Err("invalid_points");
                }
                inc(counters, "invalid_points");
                continue;
            }
            let id = p
                .track_id
                .checked_add(offset)
                .filter(|id| *id < LIMIT && !points.contains_key(id));
            let Some(id) = id else {
                if !display {
                    inc(counters, "invalid_points");
                }
                return Err("invalid_track_id");
            };
            *maximum = (*maximum).max(id);
            points.insert(
                id,
                Point {
                    track_id: id,
                    d_rel: p.d_rel.0 as f32,
                    y_rel: p.y_rel.0 as f32,
                    v_rel: p.v_rel.0 as f32,
                    v_lead: meta.v_long_ground.0 as f32,
                    a_rel: p.a_rel.0 as f32,
                    yv_rel: if p.yv_rel.0.is_finite() {
                        p.yv_rel.0 as f32
                    } else {
                        0.
                    },
                    measured: p.measured,
                    ..Point::default()
                },
            );
        }
        Ok(points)
    }
    fn convert_display(&mut self, decoded: Output) {
        match Self::points(
            decoded,
            self._display_namespace_offset,
            true,
            &mut self._display_counters,
            &mut self._display_max_track_id,
        ) {
            Ok(points) => {
                self._display_points = points;
                inc(&mut self._display_counters, "published_records");
                *self
                    ._display_counters
                    .entry("published_points".into())
                    .or_default() += self._display_points.len() as u64;
            }
            Err(key) => self.display_error(key),
        }
    }
    fn update_display(
        &mut self,
        time: f64,
        bus: i64,
        address: u32,
        data: &[u8],
        control: Option<f64>,
    ) {
        let before = strict(&self.display_decoder).rejections;
        let crc = self.display_decoder.crc_failures;
        let result = self.display_decoder.update_frame(time, bus, address, data);
        match result {
            Err(_) => self.display_error("decoder_exceptions"),
            Ok(decoded) => {
                if strict(&self.display_decoder).rejections > before
                    || self.display_decoder.crc_failures > crc
                {
                    self.display_error("invalid_record");
                } else {
                    match (control, decoded) {
                        (None, Some(_)) => self.display_error("record_mismatch"),
                        (None, None) => {}
                        (Some(time), Some(decoded)) if decoded.time_s.0 == time => {
                            self.convert_display(decoded)
                        }
                        _ => self.display_error("record_mismatch"),
                    }
                }
            }
        }
    }
    fn convert(&mut self, decoded: Output, base: &mut Base) -> Result<Data, Error> {
        match Self::points(
            decoded,
            self._namespace_offset,
            false,
            &mut self._counters,
            &mut self._max_track_id,
        ) {
            Ok(points) => {
                base.pts = points;
                inc(&mut self._counters, "published_records");
                *self._counters.entry("published_points".into()).or_default() +=
                    base.pts.len() as u64;
                Ok(Data {
                    has_display_data: true,
                    points: base.pts.values().cloned().collect(),
                    points_present: true,
                    display_points: self._display_points.values().cloned().collect(),
                    display_points_present: true,
                    ..Data::default()
                })
            }
            Err("invalid_track_id") => {
                inc(&mut self._counters, "invalid_track_id");
                Ok(self.input_error(base))
            }
            Err(_) => Err(Error::Contract("ARS510 native metadata absent")),
        }
    }
    pub fn update(
        &mut self,
        base: &mut Base,
        packets: &[Packet],
        emit: &mut impl FnMut(&str),
    ) -> Result<Option<Data>, Error> {
        let mut result = None;
        let direct = !self._inside_carrot_update;
        if direct {
            self._outer_frame += 1;
        }
        let mut now = self._receive_time_s;
        for packet in packets {
            let time = packet.mono_time as f64 * 1e-9;
            if self._packet_time_s.is_some_and(|old| time < old) {
                inc(&mut self._counters, "invalid_packet_time");
                self.input_error(base);
                self._packet_time_s = Some(time);
                result = Some(empty(true, true));
                continue;
            }
            self._packet_time_s = Some(time);
            now = Some(now.map_or(time, |now| crate::scalar::maximum(now, time)));
            if self._blocked_packet_time_s == Some(time) {
                result = Some(empty(true, true));
                continue;
            }
            self._blocked_packet_time_s = None;
            if self._record_time_s.is_some_and(|old| time - old > 0.5) {
                inc(&mut self._counters, "timeouts");
                self.reset_history(base);
                result = Some(empty(false, true));
            }
            for frame in &packet.frames {
                if self._blocked_packet_time_s == Some(time) {
                    break;
                }
                let bus = i64::from(frame.bus);
                let address = frame.address;
                let data = &frame.data;
                let summary = bus == 1 && [0x192, 0x194].contains(&address);
                let support = (bus == 0 && [0xb4, 0x24].contains(&address))
                    || (bus == 1 && [0x235, 0x237, 0x192, 0x194].contains(&address));
                let object = bus == 1 && address == 0x80;
                if !support && !object {
                    continue;
                }
                if support
                    && if summary {
                        !(4..=8).contains(&data.len())
                    } else {
                        data.len() != 8
                    }
                {
                    inc(&mut self._counters, "support_frame_length");
                    result = Some(self.input_error(base));
                    continue;
                }
                if bus == 0 && address == 0xb4 {
                    let checksum = (address
                        + (address >> 8)
                        + data.len() as u32
                        + data[..7].iter().map(|v| u32::from(*v)).sum::<u32>())
                        as u8;
                    if checksum != data[7] {
                        inc(&mut self._counters, "support_checksum_errors");
                        result = Some(self.input_error(base));
                        continue;
                    }
                }
                if object {
                    inc(&mut self._counters, "received_id80_frames");
                }
                let before = strict(&self.decoder).rejections;
                let crc = self.decoder.crc_failures;
                let completed = strict(&self.decoder)
                    .counters
                    .get("completed_records")
                    .copied()
                    .unwrap_or(0);
                let decoded = match self.decoder.update_frame(time, bus, address, data) {
                    Ok(value) => value,
                    Err(Error::ArsValue(_)) => {
                        let ambiguous = strict(&self.decoder)
                            .counters
                            .get("completed_records")
                            .copied()
                            .unwrap_or(0)
                            > completed
                            && self._completion_time_s == Some(time);
                        inc(
                            &mut self._counters,
                            if ambiguous {
                                "ambiguous_batch_time"
                            } else {
                                "invalid_native_record"
                            },
                        );
                        self.input_error(base);
                        if ambiguous {
                            self._blocked_packet_time_s = Some(time);
                        }
                        result = Some(empty(true, true));
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                if strict(&self.decoder).rejections > before || self.decoder.crc_failures > crc {
                    result = Some(self.input_error(base));
                    continue;
                }
                self.update_display(
                    time,
                    bus,
                    address,
                    data,
                    decoded.as_ref().map(|value| value.time_s.0),
                );
                if let Some(decoded) = decoded {
                    let record = decoded.time_s.0;
                    if self._completion_time_s == Some(time) || self._record_time_s == Some(record)
                    {
                        inc(&mut self._counters, "ambiguous_batch_time");
                        self.input_error(base);
                        self._blocked_packet_time_s = Some(time);
                        result = Some(empty(true, true));
                        continue;
                    }
                    if !record.is_finite() || self._record_time_s.is_some_and(|old| record < old) {
                        inc(&mut self._counters, "invalid_native_record");
                        result = Some(self.input_error(base));
                        continue;
                    }
                    self._completion_time_s = Some(time);
                    self._record_time_s = Some(record);
                    self._seen_record = true;
                    self._fault_latched = false;
                    self._unavailable = false;
                    result = Some(self.convert(decoded, base)?);
                }
            }
        }
        if let (Some(record), Some(now)) = (self._record_time_s, now) {
            if now - record > 0.5 {
                inc(&mut self._counters, "timeouts");
                self.reset_history(base);
                result = Some(empty(false, true));
            }
        }
        if self._unavailable && result.is_none() {
            inc(&mut self._counters, "no_record_updates");
        }
        if result.is_none() && direct {
            Ok(self.waiting(now, emit))
        } else {
            Ok(result)
        }
    }
    pub fn update_carrot(
        &mut self,
        base: &mut Base,
        (speed, acceleration, time): (f64, f64, f64),
        packets: &[Packet],
        numerics: &mut Numerics,
        emit: &mut impl FnMut(&str),
    ) -> Result<Option<Data>, Error> {
        if ![speed, acceleration, time].iter().all(|v| v.is_finite()) {
            inc(&mut self._counters, "nonfinite_input");
            return Ok(Some(self.input_error(base)));
        }
        if self._receive_time_s.is_some_and(|old| time < old) {
            inc(&mut self._counters, "reversed_receive_time");
            self.input_error(base);
            self._packet_time_s = None;
            self._receive_time_s = Some(time);
            return Ok(Some(empty(true, true)));
        }
        self._outer_frame += 1;
        self._start_time_s.get_or_insert(time);
        let ego = base.v_ego_hist.clone();
        let accel = base.a_ego_hist.clone();
        let mut result = None;
        self._inside_carrot_update = true;
        let outcome = (|| -> Result<(), Error> {
            for packet in packets {
                let step = packet.mono_time as f64 * 1e-9;
                self._receive_time_s = Some(step);
                if packet.frames.is_empty() {
                    base.v_ego_hist = ego.clone();
                    base.a_ego_hist = accel.clone();
                    base.push_ego(speed, acceleration)?;
                    let value = self.update(base, std::slice::from_ref(packet), emit)?;
                    retain(&mut result, base.finish(value, step, numerics, emit)?);
                }
                for frame in &packet.frames {
                    base.v_ego_hist = ego.clone();
                    base.a_ego_hist = accel.clone();
                    base.push_ego(speed, acceleration)?;
                    let packet = Packet {
                        mono_time: packet.mono_time,
                        frames: vec![frame.clone()],
                    };
                    let value = self.update(base, &[packet], emit)?;
                    retain(&mut result, base.finish(value, step, numerics, emit)?);
                }
            }
            self._receive_time_s = Some(time);
            base.v_ego_hist = ego;
            base.a_ego_hist = accel;
            base.push_ego(speed, acceleration)?;
            let value = self.update(base, &[], emit)?;
            retain(&mut result, base.finish(value, time, numerics, emit)?);
            Ok(())
        })();
        self._inside_carrot_update = false;
        outcome?;
        if result.is_none() {
            Ok(self.waiting(Some(time), emit))
        } else {
            Ok(result)
        }
    }
    pub fn diagnostics(&self, base: &Base) -> Counters {
        let mut values = self._counters.clone();
        for (key, value) in DECODER_KEYS.into_iter().zip(stats(&self.decoder)) {
            values.insert(
                key.into(),
                self._decoder_counters.get(key).copied().unwrap_or(0) + value,
            );
        }
        for (key, value) in strict(&self.decoder)
            .counters
            .iter()
            .chain(&self._assembler_counters)
        {
            values.entry(key.clone()).or_insert(0);
            *values.get_mut(key).unwrap() += value;
        }
        values.insert("namespace_offset".into(), self._namespace_offset);
        values.insert("active_points".into(), base.pts.len() as u64);
        for (key, value) in DECODER_KEYS.into_iter().zip(stats(&self.display_decoder)) {
            values.insert(
                format!("display_{key}"),
                self._display_decoder_counters
                    .get(key)
                    .copied()
                    .unwrap_or(0)
                    + value,
            );
        }
        for (key, value) in &self._display_counters {
            values.insert(format!("display_{key}"), *value);
        }
        values.insert(
            "display_namespace_offset".into(),
            self._display_namespace_offset,
        );
        values.insert(
            "active_display_points".into(),
            self._display_points.len() as u64,
        );
        for key in [
            "starts",
            "completed_records",
            "sequence_errors",
            "bad_frame_length",
            "bad_frame_type",
            "assembly_timeout",
            "nonfinite_time",
            "reversed_time",
            "interrupted_records",
            "orphan_continuations",
            "no_record_updates",
            "timeouts",
            "history_resets",
            "published_records",
            "published_points",
            "invalid_points",
            "received_id80_frames",
            "invalid_native_record",
            "invalid_packet_time",
            "nonfinite_input",
            "reversed_receive_time",
            "support_frame_length",
            "support_checksum_errors",
            "invalid_frame_data",
            "ambiguous_batch_time",
            "invalid_track_id",
            "cold_start_publications",
            "cold_start_warnings",
            "input_recovery_resets",
        ] {
            values.entry(key.into()).or_insert(0);
        }
        values
    }
}
fn retain(result: &mut Option<Data>, mut value: Option<Data>) {
    if let Some(next) = value.as_mut() {
        if result.as_ref().is_some_and(|old| old.errors.can_error)
            && next.errors.radar_unavailable_temporary
        {
            next.errors.can_error = true;
        }
        *result = value;
    }
}
