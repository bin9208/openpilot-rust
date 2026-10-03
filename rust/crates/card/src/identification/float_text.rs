use std::fmt;

struct Decimal {
    negative: bool,
    coefficient: u64,
    scale: i32,
}

impl Decimal {
    fn rounded(value: f64, precision: usize) -> Result<Self, fmt::Error> {
        let text = format!("{value:.decimals$e}", decimals = precision - 1);
        let (coefficient, exponent) = text.split_once('e').ok_or(fmt::Error)?;
        let coefficient = coefficient
            .trim_start_matches('-')
            .replace('.', "")
            .parse()
            .map_err(|_| fmt::Error)?;
        let exponent: i32 = exponent.parse().map_err(|_| fmt::Error)?;
        Ok(Self {
            negative: value.is_sign_negative(),
            coefficient,
            scale: exponent - i32::try_from(precision - 1).map_err(|_| fmt::Error)?,
        })
    }
    fn round_trips(&self, value: f64) -> Result<bool, fmt::Error> {
        let sign = if self.negative { "-" } else { "" };
        let parsed: f64 = format!("{sign}{}e{}", self.coefficient, self.scale)
            .parse()
            .map_err(|_| fmt::Error)?;
        Ok(parsed.to_bits() == value.to_bits())
    }
    fn source_text(&self) -> Result<String, fmt::Error> {
        let raw = self.coefficient.to_string();
        let digits = raw.trim_end_matches('0');
        let scale = self.scale + i32::try_from(raw.len() - digits.len()).map_err(|_| fmt::Error)?;
        let length = i32::try_from(digits.len()).map_err(|_| fmt::Error)?;
        let exponent = scale + length - 1;
        let sign = if self.negative { "-" } else { "" };
        if !(-4..16).contains(&exponent) {
            let coefficient = if digits.len() > 1 {
                format!("{}.{}", &digits[..1], &digits[1..])
            } else {
                digits.to_owned()
            };
            return Ok(format!("{sign}{coefficient}e{exponent:+03}"));
        }
        if scale >= 0 {
            let zeroes = "0".repeat(usize::try_from(scale).map_err(|_| fmt::Error)?);
            return Ok(format!("{sign}{digits}{zeroes}.0"));
        }
        let point = length + scale;
        if point > 0 {
            let point = usize::try_from(point).map_err(|_| fmt::Error)?;
            return Ok(format!("{sign}{}.{}", &digits[..point], &digits[point..]));
        }
        let zeroes = "0".repeat(usize::try_from(-point).map_err(|_| fmt::Error)?);
        Ok(format!("{sign}0.{zeroes}{digits}"))
    }
}

pub(super) fn repr(value: f64) -> Result<String, fmt::Error> {
    if value.is_nan() {
        return Ok("nan".into());
    }
    if value == f64::INFINITY {
        return Ok("inf".into());
    }
    if value == f64::NEG_INFINITY {
        return Ok("-inf".into());
    }
    if value == 0. {
        return Ok(format!("{value:?}"));
    }
    for precision in 1..=17 {
        let mut decimal = Decimal::rounded(value, precision)?;
        for coefficient in [
            decimal.coefficient,
            decimal.coefficient.saturating_sub(1),
            decimal.coefficient.checked_add(1).ok_or(fmt::Error)?,
        ] {
            decimal.coefficient = coefficient;
            if decimal.round_trips(value)? {
                return decimal.source_text();
            }
        }
    }
    Err(fmt::Error)
}

#[cfg(test)]
mod tests {
    use super::repr;

    #[test]
    fn shortest_decimal_midpoint_uses_even_last_digit() {
        assert_eq!(
            repr(13_633_225_588_252.0 + 0.0625).unwrap(),
            "13633225588252.062"
        );
    }
    #[test]
    fn source_exponent_and_fixed_notation_boundaries() {
        for (input, expected) in [
            (1e-5, "1e-05"),
            (1e-4, "0.0001"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (-0., "-0.0"),
            (5e-324, "5e-324"),
        ] {
            assert_eq!(repr(input).unwrap(), expected);
        }
    }
}
