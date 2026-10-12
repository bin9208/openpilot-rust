use crate::{Error, Value};

fn contains(body: &[u32], text: &str) -> bool {
    body.windows(text.len())
        .any(|part| part.iter().copied().eq(text.bytes().map(u32::from)))
}

fn whitespace(point: u32) -> bool {
    crate::state::trim(&[point]).is_empty()
}

fn file_changes(body: &[u32]) -> bool {
    for start in 0..body.len() {
        if !body
            .get(start)
            .is_some_and(|point| (48..=57).contains(point))
        {
            continue;
        }
        let mut end = start;
        while body.get(end).is_some_and(|point| (48..=57).contains(point)) {
            end += 1;
        }
        let before = end;
        while body.get(end).is_some_and(|point| whitespace(*point)) {
            end += 1;
        }
        if before == end
            || !body
                .get(end..)
                .is_some_and(|part| part.starts_with(&[102, 105, 108, 101]))
        {
            continue;
        }
        end += 4;
        if body.get(end) == Some(&115) {
            end += 1;
        }
        let before = end;
        while body.get(end).is_some_and(|point| whitespace(*point)) {
            end += 1;
        }
        if before != end
            && body
                .get(end..)
                .is_some_and(|part| part.starts_with(&[99, 104, 97, 110, 103, 101, 100]))
        {
            return true;
        }
    }
    false
}

pub fn did_pull_update(output: &Value) -> Result<bool, Error> {
    let Value::Text(points) = crate::param_changes::text::stripped(output, true)? else {
        return Err(Error::Source("Git pull output is not text".into()));
    };
    let mut body = Vec::new();
    for point in points {
        match char::from_u32(point) {
            Some(character) => body.extend(character.to_lowercase().map(u32::from)),
            None => body.push(point),
        }
    }
    if body.is_empty()
        || contains(&body, "already up to date")
        || contains(&body, "already up-to-date")
    {
        return Ok(false);
    }
    Ok(contains(&body, "fast-forward")
        || contains(&body, "merge made by")
        || contains(&body, "updating ")
        || file_changes(&body))
}
