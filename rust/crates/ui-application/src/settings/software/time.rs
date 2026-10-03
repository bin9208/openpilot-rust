use crate::{context::Context, params::datetime::DateTime};
use chrono::Datelike;
pub(super) fn ago(
    context: &Context,
    date: DateTime,
) -> Result<String, openpilot_ui_framework::Error> {
    let valid = context
        .system_time_valid()
        .map_err(|error| openpilot_ui_framework::Error::Io(std::io::Error::other(error)))?;
    if valid {
        let now = (context.now_wall)().naive_utc();
        let elapsed = now
            .signed_duration_since(date.local)
            .num_microseconds()
            .ok_or(openpilot_ui_framework::Error::Contract(
                "software date interval overflow",
            ))?
            + date.offset_micros;
        let seconds = elapsed / 1_000_000;
        if seconds < 60 {
            return Ok(context.tr("now"));
        }
        for (limit, divisor, singular, plural) in [
            (3600, 60, "{} minute ago", "{} minutes ago"),
            (86400, 3600, "{} hour ago", "{} hours ago"),
            (604800, 86400, "{} day ago", "{} days ago"),
        ] {
            if seconds < limit {
                let count = seconds / divisor;
                return Ok(context
                    .trn(singular, plural, count)
                    .replace("{}", &count.to_string()));
            }
        }
    }
    Ok(format!(
        "{} {}",
        date.local.format("%a %b %d"),
        date.local.year()
    ))
}
