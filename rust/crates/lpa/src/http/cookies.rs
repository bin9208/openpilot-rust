pub(super) fn header<'a>(cookies: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut result = String::new();
    for (name, value) in cookies {
        if !result.is_empty() {
            result.push_str("; ");
        }
        result.push_str(name);
        result.push('=');
        result.push_str(value);
    }
    result
}
