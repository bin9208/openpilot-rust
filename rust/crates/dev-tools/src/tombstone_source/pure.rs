//! Literal source safe_fn and report-filename semantics, including Python types.
use super::{value as v, Error, Result};
use openpilot_log_format::{JsonValue as J, JsonView};
use unicode_general_category::{get_general_category, GeneralCategory};
fn alnum(points: &[u32]) -> bool {
    !points.is_empty()
        && points.iter().all(|&point| {
            char::from_u32(point).is_some_and(|point| {
                matches!(
                    get_general_category(point),
                    GeneralCategory::UppercaseLetter
                        | GeneralCategory::LowercaseLetter
                        | GeneralCategory::TitlecaseLetter
                        | GeneralCategory::ModifierLetter
                        | GeneralCategory::OtherLetter
                        | GeneralCategory::DecimalNumber
                        | GeneralCategory::LetterNumber
                        | GeneralCategory::OtherNumber
                )
            })
        })
}
pub(super) fn safe(value: &J) -> Result<J> {
    let items = match value.view() {
        JsonView::Text(points) => points.iter().map(|&point| v::text(vec![point])).collect(),
        JsonView::Array(values) => values,
        JsonView::Object(fields) => fields
            .into_iter()
            .map(|(key, _)| v::text(key.to_vec()))
            .collect(),
        _ => return Err(Error::new("TypeError", "safe_fn input is not iterable")),
    };
    let mut out = Vec::new();
    for item in items {
        let points = v::points(&item, "AttributeError")?;
        if alnum(&points) || points == [95] {
            out.extend(points);
        }
    }
    // Every admitted item is alphanumeric or exactly '_'; none has trailing
    // whitespace, so the source rstrip() leaves these joined points unchanged.
    Ok(v::text(out))
}
pub(super) fn filename(request: &J) -> Result<J> {
    // Worker.handle performs json.loads(commit), constructs source metadata,
    // then fetches path/date before evaluating the two original assignments.
    let commit = v::field(request, "commit")?;
    let commit = v::parse(&v::points(&commit, "TypeError")?)?;
    let path = v::field(request, "path")?;
    let date = v::field(request, "stamp")?;
    let mut path = v::points(&path, "AttributeError")?;
    for point in &mut path {
        if *point == 47 {
            *point = 95;
        }
    }
    let commit = if v::truth(&commit) {
        commit
    } else {
        J::text("nocommit")
    };
    let prefix = match commit.view() {
        JsonView::Text(points) => v::text(points[..points.len().min(8)].to_vec()),
        JsonView::Array(values) => v::array(&values[..values.len().min(8)]),
        // Python3.12 slices are hashable. dict[ :8 ] is therefore a KeyError,
        // as independently observed, rather than Python3.11's unhashable slice.
        JsonView::Object(_) => return Err(Error::new("KeyError", "slice(None, 8, None)")),
        _ => return Err(Error::new("TypeError", "commit is not subscriptable")),
    };
    let safe = safe(&v::text(path))?;
    let mut out = v::string(&date);
    out.push(95);
    out.extend(v::string(&prefix));
    out.push(95);
    out.extend(v::string(&safe));
    out.truncate(62);
    Ok(v::text(out))
}
