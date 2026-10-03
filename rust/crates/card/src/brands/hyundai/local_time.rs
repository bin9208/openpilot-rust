mod posix;
use super::{
    wire::{get, Values},
    Error,
};
use chrono::NaiveDate;
use num_traits::ToPrimitive;

pub fn country_zone(country: i32) -> &'static str {
    match country {
        840 => "America/New_York",
        124 => "America/Toronto",
        250 => "Europe/Paris",
        276 => "Europe/Berlin",
        826 => "Europe/London",
        392 => "Asia/Tokyo",
        156 => "Asia/Shanghai",
        410 => "Asia/Seoul",
        36 => "Australia/Sydney",
        356 => "Asia/Kolkata",
        _ => "UTC",
    }
}

fn number(data: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .ok_or(Error::Numeric)?
        .try_into()
        .map_err(|_| Error::Numeric)?;
    Ok(u32::from_be_bytes(bytes))
}

struct Header {
    time: usize,
    types: usize,
    chars: usize,
    leap: usize,
    std: usize,
    gmt: usize,
}
impl Header {
    fn read(data: &[u8], start: usize) -> Result<Self, Error> {
        let count =
            |offset| usize::try_from(number(data, start + offset)?).map_err(|_| Error::Numeric);
        Ok(Self {
            gmt: count(20)?,
            std: count(24)?,
            leap: count(28)?,
            time: count(32)?,
            types: count(36)?,
            chars: count(40)?,
        })
    }
    fn size(&self, width: usize) -> usize {
        self.time * (width + 1)
            + self.types * 6
            + self.chars
            + self.leap * (width + 4)
            + self.std
            + self.gmt
    }
}

pub struct Zone {
    transitions: Vec<(i64, usize)>,
    offsets: Vec<i32>,
    future: Option<posix::Future>,
}

impl Zone {
    pub fn load(name: &str) -> Result<Self, Error> {
        let bytes = std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(name))?;
        if bytes.get(..4) != Some(b"TZif") {
            return Err(Error::Numeric);
        }
        let first = Header::read(&bytes, 0)?;
        let version = *bytes.get(4).ok_or(Error::Numeric)?;
        let (start, width) = if version == b'2' || version == b'3' || version == b'4' {
            (44 + first.size(4), 8)
        } else {
            (0, 4)
        };
        let header = Header::read(&bytes, start)?;
        let mut transitions = Vec::with_capacity(header.time);
        let times = start + 44;
        let indices = times + header.time * width;
        for index in 0..header.time {
            let offset = times + index * width;
            let time = if width == 8 {
                i64::from_be_bytes(
                    bytes
                        .get(offset..offset + 8)
                        .ok_or(Error::Numeric)?
                        .try_into()
                        .map_err(|_| Error::Numeric)?,
                )
            } else {
                i64::from(i32::from_be_bytes(
                    bytes
                        .get(offset..offset + 4)
                        .ok_or(Error::Numeric)?
                        .try_into()
                        .map_err(|_| Error::Numeric)?,
                ))
            };
            transitions.push((
                time,
                usize::from(*bytes.get(indices + index).ok_or(Error::Numeric)?),
            ));
        }
        let types = indices + header.time;
        let mut offsets = Vec::with_capacity(header.types);
        for index in 0..header.types {
            let offset = types + index * 6;
            offsets.push(i32::from_be_bytes(
                bytes
                    .get(offset..offset + 4)
                    .ok_or(Error::Numeric)?
                    .try_into()
                    .map_err(|_| Error::Numeric)?,
            ));
        }
        let tail = start + 44 + header.size(width);
        let future = if width == 8 {
            std::str::from_utf8(bytes.get(tail..).ok_or(Error::Numeric)?)?.trim_matches('\n')
        } else {
            ""
        };
        Ok(Self {
            transitions,
            offsets,
            future: if future.is_empty() {
                None
            } else {
                Some(posix::Future::parse(future)?)
            },
        })
    }

    fn offset(&self, epoch: i64) -> Result<i32, Error> {
        if self
            .transitions
            .last()
            .is_none_or(|(last, _)| epoch > *last)
        {
            if let Some(future) = &self.future {
                return future.offset(epoch);
            }
        }
        let index = self
            .transitions
            .partition_point(|(transition, _)| *transition <= epoch);
        let kind = if index == 0 {
            0
        } else {
            self.transitions.get(index - 1).ok_or(Error::Numeric)?.1
        };
        self.offsets.get(kind).copied().ok_or(Error::Numeric)
    }

    pub fn local_epoch(&self, local: i64) -> Result<i64, Error> {
        let mut candidates = Vec::new();
        for offset in self
            .offsets
            .iter()
            .copied()
            .chain(self.future.iter().flat_map(posix::Future::offsets))
        {
            let epoch = local - i64::from(offset);
            if self.offset(epoch)? == offset {
                candidates.push(epoch);
            }
        }
        if let Some(first) = candidates.into_iter().min() {
            return Ok(first);
        }
        for (transition, kind) in &self.transitions {
            let before = self.offset(transition - 1)?;
            let after = *self.offsets.get(*kind).ok_or(Error::Numeric)?;
            if after > before
                && local >= transition + i64::from(before)
                && local < transition + i64::from(after)
            {
                return Ok(local - i64::from(before));
            }
        }
        if let Some(future) = &self.future {
            return future.gap_epoch(local);
        }
        Err(Error::Numeric)
    }
}

pub fn timestamp(zone: &Zone, data: &Values) -> Result<Option<u64>, Error> {
    let year = get(data, "YEAR")?.to_i32().ok_or(Error::Numeric)? + 2000;
    let month = get(data, "MONTH")?.to_u32().ok_or(Error::Numeric)?;
    let day = get(data, "DATE")?.to_u32().ok_or(Error::Numeric)?;
    let hour = get(data, "HOURS")?.to_u32().ok_or(Error::Numeric)?;
    let minute = get(data, "MINUTES")?.to_u32().ok_or(Error::Numeric)?;
    let second = get(data, "SECONDS")?.to_u32().ok_or(Error::Numeric)?;
    let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|date| date.and_hms_opt(hour, minute, second))
    else {
        return Ok(None);
    };
    let epoch = zone.local_epoch(date.and_utc().timestamp())?;
    Ok(Some(
        u64::try_from(epoch.checked_mul(1000).ok_or(Error::Numeric)?)
            .map_err(|_| Error::Numeric)?,
    ))
}
