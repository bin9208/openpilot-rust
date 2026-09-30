//! The repository's deliberately small release-note renderer, not CommonMark.
fn space(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\x1c'..='\x1f')
}
fn close_lists(output: &mut Vec<String>, mut level: usize, end: usize) -> usize {
    while level > end {
        level -= 1;
        output.push("</ul>".into());
        if level > 0 {
            output.push("</li>".into());
        }
    }
    end
}
pub fn parse(text: &str, tab_length: usize) -> Result<String, crate::Error> {
    if tab_length == 0 {
        return Err(crate::Error::Contract("zero markdown tab length"));
    }
    let indent = " ".repeat(tab_length);
    let lines: Vec<_> = text.split('\n').collect();
    let mut output: Vec<String> = Vec::new();
    let mut level = 0;
    for (index, line) in lines.iter().enumerate() {
        if lines
            .get(index + 1)
            .is_some_and(|line| line.starts_with("==="))
        {
            output.push(format!("<h1>{line}</h1>"));
        } else if line.starts_with("===") {
            continue;
        } else if line.trim_start_matches(space).starts_with("* ") {
            let star = line
                .find('*')
                .ok_or(crate::Error::Contract("list marker absent"))?;
            let line_level = 1 + line[..star].matches(&indent).count();
            if level >= line_level {
                level = close_lists(&mut output, level, line_level);
            } else {
                level += 1;
                if level > 1 {
                    let last = output
                        .last_mut()
                        .ok_or(crate::Error::Contract("nested list without parent"))?;
                    *last = last.replace("</li>", "");
                }
                output.push("<ul>".into());
            }
            output.push(format!(
                "<li>{}</li>",
                line.replacen('*', "", 1).trim_start_matches(space)
            ));
        } else {
            level = close_lists(&mut output, level, 0);
            if !line.is_empty() {
                output.push((*line).into());
            }
        }
    }
    close_lists(&mut output, level, 0);
    Ok((output.join("\n") + "\n")
        .replace('&', "&amp;")
        .replace('"', "&quot;"))
}
pub fn release_notes(path: &std::path::Path) -> Result<Vec<u8>, crate::Error> {
    let data = match std::fs::read(path.join("RELEASES.md")) {
        Ok(data) => data,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let end = data
        .windows(2)
        .position(|v| v == b"\n\n")
        .unwrap_or(data.len());
    let bytes = &data[..end];
    match std::str::from_utf8(bytes) {
        Ok(text) => match parse(text, 2) {
            Ok(html) => Ok(html.into_bytes()),
            Err(_) => Ok([bytes, b"\n"].concat()),
        },
        Err(_) => Ok([bytes, b"\n"].concat()),
    }
}
