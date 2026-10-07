use crate::brands::hyundai::Error;
use chrono::{Datelike, NaiveDate};

pub struct Future {
    standard: i32,
    dst: Option<(i32, Rule, Rule)>,
}
pub struct Rule {
    month: u32,
    week: u32,
    day: u32,
    seconds: i32,
}

fn seconds(raw: &str) -> Result<i32, Error> {
    let sign = if raw.starts_with('-') { -1 } else { 1 };
    let parts = raw
        .trim_start_matches(['+', '-'])
        .split(':')
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()?;
    let hour = *parts.first().ok_or(Error::Numeric)?;
    Ok(sign
        * (hour * 3600
            + parts.get(1).copied().unwrap_or(0) * 60
            + parts.get(2).copied().unwrap_or(0)))
}

fn abbreviation(raw: &str) -> Result<&str, Error> {
    if raw.starts_with('<') {
        return raw
            .find('>')
            .and_then(|index| raw.get(index + 1..))
            .ok_or(Error::Numeric);
    }
    let length = raw.bytes().take_while(u8::is_ascii_alphabetic).count();
    if length == 0 {
        return Err(Error::Numeric);
    }
    raw.get(length..).ok_or(Error::Numeric)
}

fn offset(raw: &str) -> Result<(i32, &str), Error> {
    let length = raw
        .bytes()
        .take_while(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b':'))
        .count();
    Ok((
        seconds(raw.get(..length).ok_or(Error::Numeric)?)?,
        raw.get(length..).ok_or(Error::Numeric)?,
    ))
}

impl Rule {
    fn parse(raw: &str) -> Result<Self, Error> {
        let (calendar, time) = raw.split_once('/').unwrap_or((raw, "2"));
        let parts = calendar
            .strip_prefix('M')
            .ok_or(Error::Numeric)?
            .split('.')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            month: *parts.first().ok_or(Error::Numeric)?,
            week: *parts.get(1).ok_or(Error::Numeric)?,
            day: *parts.get(2).ok_or(Error::Numeric)?,
            seconds: seconds(time)?,
        })
    }
    fn local(&self, year: i32) -> Result<i64, Error> {
        let first = NaiveDate::from_ymd_opt(year, self.month, 1).ok_or(Error::Numeric)?;
        let weekday = first.weekday().num_days_from_sunday();
        let mut day = 1 + (self.day + 7 - weekday) % 7 + (self.week - 1) * 7;
        if NaiveDate::from_ymd_opt(year, self.month, day).is_none() {
            day -= 7;
        }
        let date = NaiveDate::from_ymd_opt(year, self.month, day)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .ok_or(Error::Numeric)?;
        Ok(date.and_utc().timestamp() + i64::from(self.seconds))
    }
}

impl Future {
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let raw = abbreviation(raw)?;
        let (standard, raw) = offset(raw)?;
        let standard = -standard;
        if raw.is_empty() {
            return Ok(Self {
                standard,
                dst: None,
            });
        }
        let raw = abbreviation(raw)?;
        let (dst, raw) = if raw.starts_with(',') {
            (standard + 3600, raw)
        } else {
            let (offset, raw) = offset(raw)?;
            (-offset, raw)
        };
        let rules = raw
            .strip_prefix(',')
            .ok_or(Error::Numeric)?
            .split(',')
            .collect::<Vec<_>>();
        Ok(Self {
            standard,
            dst: Some((
                dst,
                Rule::parse(rules.first().ok_or(Error::Numeric)?)?,
                Rule::parse(rules.get(1).ok_or(Error::Numeric)?)?,
            )),
        })
    }
    pub fn offsets(&self) -> Vec<i32> {
        match &self.dst {
            Some((dst, _, _)) => vec![self.standard, *dst],
            None => vec![self.standard],
        }
    }
    pub fn offset(&self, epoch: i64) -> Result<i32, Error> {
        let Some((dst, start, end)) = &self.dst else {
            return Ok(self.standard);
        };
        let year = chrono::DateTime::from_timestamp(epoch, 0)
            .ok_or(Error::Numeric)?
            .year();
        let start = start.local(year)? - i64::from(self.standard);
        let end = end.local(year)? - i64::from(*dst);
        let active = if start < end {
            epoch >= start && epoch < end
        } else {
            epoch >= start || epoch < end
        };
        Ok(if active { *dst } else { self.standard })
    }
    pub fn gap_epoch(&self, local: i64) -> Result<i64, Error> {
        let year = chrono::DateTime::from_timestamp(local, 0)
            .ok_or(Error::Numeric)?
            .year();
        if let Some((dst, start, _)) = &self.dst {
            let start = start.local(year)?;
            if local >= start && local < start + i64::from(*dst - self.standard) {
                return Ok(local - i64::from(self.standard));
            }
        }
        Err(Error::Numeric)
    }
}
