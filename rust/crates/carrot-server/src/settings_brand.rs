use crate::{
    json_fields::{array, field, fields, key, set},
    settings::Catalog,
    settings_menu::count_items,
    Error, Value,
};

impl Catalog {
    pub fn for_brand(&self, brand: &str) -> Result<Self, Error> {
        let brand = brand.trim().to_lowercase();
        let mut hidden = Vec::new();
        if !brand.is_empty() {
            for (_, items) in fields(&self.groups)? {
                for item in array(items)? {
                    if !item.has("hidden_brands") {
                        continue;
                    }
                    for candidate in array(item.get("hidden_brands"))? {
                        if candidate.string()?.trim().to_lowercase() == brand {
                            hidden.push(item.get("name").py_string()?);
                            break;
                        }
                    }
                }
            }
        }
        if hidden.is_empty() {
            return Ok(self.clone());
        }
        let mut result = self.clone();
        if let Value::Object(groups) = &mut result.groups {
            for (_, items) in groups {
                *items = Value::Array(
                    array(items)?
                        .iter()
                        .filter(|item| !hidden.contains(item.get("name")))
                        .cloned()
                        .collect(),
                );
            }
        }
        let mut group_list = array(&result.groups_list)?.clone();
        for group in &mut group_list {
            let group_key = key(group.get("group"))?;
            let count = match field(fields(&result.groups)?, &group_key) {
                Some(items) => array(items)?
                    .iter()
                    .filter(|item| !item.get("detail_parent").truth())
                    .count(),
                None => 0,
            };
            set(group, "count", Value::integer(count))?;
        }
        result.groups_list = Value::Array(group_list);
        if !matches!(result.categories, Value::Null) {
            let mut categories = array(&result.categories)?.clone();
            for category in &mut categories {
                let mut groups = array(category.get("groups"))?.clone();
                for group in &mut groups {
                    let mut section_list = array(group.get("sections"))?.clone();
                    for section in &mut section_list {
                        let items = array(section.get("items"))?
                            .iter()
                            .filter(|name| !hidden.contains(name))
                            .cloned()
                            .collect();
                        set(section, "items", Value::Array(items))?;
                    }
                    section_list.retain(|section| section.get("items").truth());
                    let count = count_items(&section_list, &result.by_name)?;
                    set(group, "sections", Value::Array(section_list))?;
                    set(group, "count", Value::integer(count))?;
                }
                groups.retain(|group| group.get("count").truth());
                set(category, "groups", Value::Array(groups))?;
            }
            result.categories = Value::Array(categories);
        }
        Ok(result)
    }
}
