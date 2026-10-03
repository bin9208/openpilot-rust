pub fn buffers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if key == "buffer" {
                    if let serde_json::Value::Array(values) = value {
                        let mut runs = Vec::new();
                        let mut previous = None;
                        let mut count = 0u64;
                        for value in values.iter() {
                            if previous == Some(value) {
                                count += 1;
                            } else {
                                if let Some(value) = previous {
                                    runs.push(serde_json::json!([count, value]));
                                }
                                previous = Some(value);
                                count = 1;
                            }
                        }
                        if let Some(value) = previous {
                            runs.push(serde_json::json!([count, value]));
                        }
                        *values = runs;
                    }
                } else {
                    buffers(value);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                buffers(value);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => (),
    }
}
