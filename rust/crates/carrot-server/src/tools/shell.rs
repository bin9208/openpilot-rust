//! Python shlex.split(comments=False,posix=True) and shlex.quote at the Tools boundary.
#[derive(Clone, Copy)]
enum Quote {
    None,
    Single,
    Double,
}
pub(super) fn split(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut opened = false;
    let mut quote = Quote::None;
    let mut chars = input.chars();
    while let Some(character) = chars.next() {
        match quote {
            Quote::Single => {
                if character == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(character);
                }
            }
            Quote::Double => match character {
                '"' => quote = Quote::None,
                '\\' => {
                    let next = chars.next()?;
                    if !matches!(next, '"' | '\\') {
                        word.push('\\');
                    }
                    word.push(next);
                }
                character => word.push(character),
            },
            Quote::None => match character {
                ' ' | '\t' | '\n' | '\r' => {
                    if opened {
                        words.push(std::mem::take(&mut word));
                        opened = false;
                    }
                }
                '\'' => {
                    opened = true;
                    quote = Quote::Single;
                }
                '"' => {
                    opened = true;
                    quote = Quote::Double;
                }
                '\\' => {
                    opened = true;
                    word.push(chars.next()?);
                }
                character => {
                    opened = true;
                    word.push(character);
                }
            },
        }
    }
    match quote {
        Quote::Single | Quote::Double => None,
        Quote::None => {
            if opened {
                words.push(word);
            }
            Some(words)
        }
    }
}
pub(super) fn quote(input: &str) -> String {
    if !input.is_empty()
        && input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
    {
        return input.into();
    }
    format!("'{}'", input.replace('\'', "'\"'\"'"))
}
