use crate::{
    json_fields::{array, field, fields, key, set},
    Error, Value,
};

fn label(node: &Value) -> Value {
    Value::object([
        ("ko", node.get("ko").clone()),
        ("en", node.get("en").clone()),
        ("zh", node.get("zh").clone()),
    ])
}

fn leaf_items(node: &Value, by_name: &Value) -> Result<Vec<Value>, Error> {
    let empty = Value::Array(Vec::new());
    let source = if node.has("params") {
        node.get("params")
    } else {
        &empty
    };
    let names = fields(by_name)?;
    let mut output = Vec::new();
    for name in array(source)? {
        if field(names, &key(name)?).is_some() {
            output.push(name.clone());
        }
    }
    Ok(output)
}

fn sections(nodes: &[Value], parents: &[Value], by_name: &Value) -> Result<Vec<Value>, Error> {
    let mut output = Vec::new();
    for node in nodes {
        fields(node)?;
        let mut path = parents.to_vec();
        path.push(node.clone());
        if node.get("groups").truth() {
            output.extend(sections(array(node.get("groups"))?, &path, by_name)?);
            continue;
        }
        let items = leaf_items(node, by_name)?;
        if items.is_empty() {
            continue;
        }
        let join = |name: &str| -> Result<Value, Error> {
            let mut parts = Vec::new();
            for part in &path {
                if part.get(name).truth() {
                    let text = part.get(name).string()?.trim().to_owned();
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
            }
            Ok(if parts.is_empty() {
                Value::Null
            } else {
                Value::text(&parts.join(" · "))
            })
        };
        let mut ids = Vec::new();
        for part in &path {
            if part.get("id").truth() {
                ids.push(part.get("id").string()?);
            }
        }
        output.push(Value::object([
            ("id", Value::text(&ids.join("__"))),
            ("ko", join("ko")?),
            ("en", join("en")?),
            ("zh", join("zh")?),
            ("items", Value::Array(items)),
        ]));
    }
    Ok(output)
}

pub(crate) fn count_items(sections: &[Value], by_name: &Value) -> Result<usize, Error> {
    let mut count = 0;
    for section in sections {
        for name in array(section.get("items"))? {
            if let Some(item) = field(fields(by_name)?, &key(name)?) {
                if !item.get("detail_parent").truth() {
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

pub(crate) fn build_menu(data: &Value, by_name: &Value) -> Result<Value, Error> {
    if !data.get("menu").truth() {
        return Ok(Value::Null);
    }
    let mut categories = Vec::new();
    for category in array(data.get("menu"))? {
        fields(category)?;
        let empty = Value::Array(Vec::new());
        let groups = if category.has("groups") {
            category.get("groups")
        } else {
            &empty
        };
        let mut output_groups = Vec::new();
        for group in array(groups)? {
            fields(group)?;
            let section_items = if group.has("groups") {
                sections(array(group.get("groups"))?, &[], by_name)?
            } else {
                vec![Value::object([
                    ("id", group.get("id").clone()),
                    ("ko", Value::Null),
                    ("en", Value::Null),
                    ("zh", Value::Null),
                    ("items", Value::Array(leaf_items(group, by_name)?)),
                ])]
            };
            let mut result = label(group);
            set(&mut result, "id", group.get("id").clone())?;
            set(
                &mut result,
                "count",
                Value::integer(count_items(&section_items, by_name)?),
            )?;
            set(&mut result, "sections", Value::Array(section_items))?;
            output_groups.push(result);
        }
        let mut result = label(category);
        set(&mut result, "id", category.get("id").clone())?;
        set(&mut result, "groups", Value::Array(output_groups))?;
        categories.push(result);
    }
    Ok(Value::Array(categories))
}
