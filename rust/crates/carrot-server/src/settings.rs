pub use crate::settings_cache::SettingsCache;
use crate::{
    json_fields::{array, fields, insert, key, set},
    settings_menu::build_menu,
};
use crate::{Error, Value};

#[derive(Clone, Debug)]
pub struct Catalog {
    pub data: Value,
    pub groups: Value,
    pub by_name: Value,
    pub groups_list: Value,
    pub categories: Value,
}

impl Catalog {
    pub fn from_data(mut data: Value) -> Result<Self, Error> {
        fields(&data)?;
        let mut groups: Vec<(Vec<u32>, Value)> = Vec::new();
        let mut group_labels = Vec::new();
        let mut names = Vec::new();
        if data.has("params") {
            let mut definitions = array(data.get("params"))?.clone();
            for item in &mut definitions {
                fields(item)?;
                let group = if item.has("group") {
                    item.get("group").clone()
                } else {
                    Value::text("기타")
                };
                if group.text_eq("기타") {
                    if !item.has("egroup") {
                        set(item, "egroup", Value::text("Other"))?;
                    }
                    if !item.has("cgroup") {
                        set(item, "cgroup", Value::text("其他"))?;
                    }
                }
                let group_key = key(&group)?;
                if let Some((_, Value::Array(items))) =
                    groups.iter_mut().find(|(name, _)| *name == group_key)
                {
                    items.push(item.clone());
                } else {
                    group_labels.push(group);
                    groups.push((group_key, Value::Array(vec![item.clone()])));
                }
                if item.get("name").truth() {
                    insert(&mut names, key(item.get("name"))?, item.clone());
                }
            }
            set(&mut data, "params", Value::Array(definitions))?;
        }
        let mut groups_list = Vec::new();
        for ((_, values), group) in groups.iter().zip(group_labels) {
            let items = array(values)?;
            let first = |name: &str| {
                items
                    .iter()
                    .map(|item| item.get(name))
                    .find(|value| value.truth())
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            groups_list.push(Value::object([
                ("group", group),
                ("egroup", first("egroup")),
                ("cgroup", first("cgroup")),
                (
                    "count",
                    Value::integer(
                        items
                            .iter()
                            .filter(|item| !item.get("detail_parent").truth())
                            .count(),
                    ),
                ),
            ]));
        }
        let by_name = Value::Object(names);
        let categories = build_menu(&data, &by_name)?;
        Ok(Self {
            data,
            groups: Value::Object(groups),
            by_name,
            groups_list: Value::Array(groups_list),
            categories,
        })
    }

    pub fn with_gap_limits(&self, maximum: i64) -> Result<Self, Error> {
        if !self.by_name.has("CruiseGapLevels") {
            return Ok(self.clone());
        }
        let maximum = if matches!(maximum, 3 | 4) { maximum } else { 3 };
        let mut setting = self.by_name.get("CruiseGapLevels").clone();
        set(&mut setting, "max", Value::integer(maximum))?;
        set(&mut setting, "default", Value::integer(maximum))?;
        if !setting.has("options") {
            return Err(Error::Source("'options'".into()));
        }
        let mut options = fields(setting.get("options"))?.clone();
        for (_, option) in &mut options {
            let choices = array(option)?;
            *option = Value::Array(
                choices
                    .iter()
                    .take(usize::try_from(maximum - 1).unwrap_or(2))
                    .cloned()
                    .collect(),
            );
        }
        set(&mut setting, "options", Value::Object(options))?;
        let adapt = |items: &Value| -> Result<Value, Error> {
            Ok(Value::Array(
                array(items)?
                    .iter()
                    .map(|item| {
                        if item.get("name").text_eq("CruiseGapLevels") {
                            setting.clone()
                        } else {
                            item.clone()
                        }
                    })
                    .collect(),
            ))
        };
        let mut result = self.clone();
        if result.data.has("params") {
            let params = adapt(result.data.get("params"))?;
            set(&mut result.data, "params", params)?;
        }
        if let Value::Object(groups) = &mut result.groups {
            for (_, items) in groups {
                *items = adapt(items)?;
            }
        }
        set(&mut result.by_name, "CruiseGapLevels", setting)?;
        Ok(result)
    }

    pub fn payload(&self, path: &std::path::Path, has_params: bool) -> Value {
        Value::object([
            ("path", Value::text(&path.to_string_lossy())),
            ("apilot", self.data.get("apilot").clone()),
            ("groups", self.groups_list.clone()),
            ("items_by_group", self.groups.clone()),
            ("categories", self.categories.clone()),
            (
                "unit_cycle",
                Value::Array(
                    crate::config::UNIT_CYCLE
                        .into_iter()
                        .map(Value::integer)
                        .collect(),
                ),
            ),
            ("has_params", Value::Bool(has_params)),
            ("has_param_type", Value::Bool(has_params)),
        ])
    }
}
