//! Compiler providers reached through this group's local Cargo features.
//! Registry feature resolution remains Cargo's job. Local target conditions are
//! rejected until explicitly supported; the current workspace has none.
use crate::{catalog::Group, Result};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Deserialize)]
struct Metadata {
    workspace_members: Vec<String>,
    packages: Vec<Package>,
}
#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    features: BTreeMap<String, Vec<String>>,
    dependencies: Vec<Dependency>,
}
#[derive(Deserialize)]
struct Dependency {
    name: String,
    rename: Option<String>,
    kind: Option<String>,
    path: Option<PathBuf>,
    target: Option<String>,
    optional: bool,
    uses_default_features: bool,
    features: Vec<String>,
}
impl Dependency {
    fn alias(&self) -> &str {
        self.rename.as_deref().unwrap_or(&self.name)
    }
    fn compiled(&self) -> bool {
        self.kind.as_deref() != Some("dev")
    }
}
pub(crate) struct Workspace {
    packages: BTreeMap<String, Package>,
}
impl Workspace {
    pub(crate) fn read(value: &Value) -> Result<Self> {
        let metadata: Metadata = serde_json::from_value(value.clone())?;
        let members: BTreeSet<_> = metadata.workspace_members.into_iter().collect();
        let mut packages = BTreeMap::new();
        for package in metadata.packages {
            if members.contains(&package.id)
                && packages.insert(package.name.clone(), package).is_some()
            {
                return Err("compiler input audit requires unique workspace package names".into());
            }
        }
        Ok(Self { packages })
    }
    pub(crate) fn requirements(&self, group: &Group) -> Result<BTreeMap<String, PathBuf>> {
        let mut active: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut enabled: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut build_context = BTreeMap::new();
        for name in &group.packages {
            build_context.insert(name.clone(), false);
            let package = self
                .packages
                .get(name)
                .ok_or("compiler input audit selected an absent package")?;
            let features = active.entry(name.clone()).or_default();
            if package.features.contains_key("default") {
                features.insert("default".into());
            }
        }
        for feature in &group.features {
            let (package, feature) = feature
                .split_once('/')
                .ok_or("unqualified catalog feature")?;
            active
                .get_mut(package)
                .ok_or("catalog feature package was not selected")?
                .insert(feature.into());
        }
        loop {
            let before = (active.clone(), enabled.clone());
            for (name, features) in active.clone() {
                let package = &self.packages[&name];
                let on = enabled.entry(name.clone()).or_default();
                let mut requested: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
                let mut pending: Vec<_> = features.into_iter().collect();
                let mut expanded = BTreeSet::new();
                while let Some(feature) = pending.pop() {
                    if !expanded.insert(feature.clone()) {
                        continue;
                    }
                    active
                        .get_mut(&name)
                        .ok_or("missing active feature state")?
                        .insert(feature.clone());
                    let aliases = package
                        .features
                        .get(&feature)
                        .ok_or("unknown local Cargo feature in compiler input audit")?;
                    for alias in aliases {
                        if let Some(dependency) = alias.strip_prefix("dep:") {
                            if !package.dependencies.iter().any(|d| d.alias() == dependency) {
                                return Err("unknown feature dependency".into());
                            }
                            on.insert(dependency.into());
                        } else if let Some((dependency, feature)) = alias.split_once('/') {
                            let weak = dependency.ends_with('?');
                            let dependency = dependency.strip_suffix('?').unwrap_or(dependency);
                            if feature.is_empty()
                                || !package.dependencies.iter().any(|d| d.alias() == dependency)
                            {
                                return Err("unknown feature dependency forwarding".into());
                            }
                            if !weak {
                                on.insert(dependency.into());
                            }
                            if on.contains(dependency) {
                                requested
                                    .entry(dependency.into())
                                    .or_default()
                                    .insert(feature.into());
                            }
                        } else {
                            pending.push(alias.clone());
                        }
                    }
                }
                for dependency in package.dependencies.iter().filter(|d| d.compiled()) {
                    let alias = dependency.alias();
                    if dependency.optional && !on.contains(alias) {
                        continue;
                    }
                    on.insert(alias.into());
                    if let Some(target) = dependency
                        .path
                        .as_ref()
                        .and_then(|_| self.packages.get(&dependency.name))
                    {
                        if dependency.target.is_some() {
                            return Err("conditional local dependency requires an explicit compiler input policy".into());
                        }
                        let context =
                            build_context[&name] || dependency.kind.as_deref() == Some("build");
                        if build_context
                            .get(&target.name)
                            .is_some_and(|old| *old != context)
                        {
                            return Err("mixed local build/runtime feature contexts require an explicit compiler input policy".into());
                        }
                        build_context.insert(target.name.clone(), context);
                        let features = active.entry(target.name.clone()).or_default();
                        if dependency.uses_default_features
                            && target.features.contains_key("default")
                        {
                            features.insert("default".into());
                        }
                        for feature in dependency
                            .features
                            .iter()
                            .chain(requested.get(alias).into_iter().flatten())
                        {
                            if !target.features.contains_key(feature) {
                                return Err(
                                    "forwarded local feature is absent from Cargo metadata".into(),
                                );
                            }
                            features.insert(feature.clone());
                        }
                    } else if dependency.path.is_some() && dependency.name.starts_with("openpilot-")
                    {
                        return Err(
                            "local compiler dependency is absent from workspace metadata".into(),
                        );
                    }
                }
            }
            if before == (active.clone(), enabled.clone()) {
                break;
            }
        }
        let has = |package: &str, feature: &str| {
            active.get(package).is_some_and(|set| set.contains(feature))
        };
        let mut result =
            BTreeMap::from([("OPENPILOT_CARGO_VENDOR".into(), PathBuf::from("vendor"))]);
        if active.contains_key("openpilot-cereal") {
            result.insert("OPENPILOT_CAPNP".into(), "usr/bin/capnp".into());
        }
        let mut ffmpeg = false;
        for (name, on) in &enabled {
            for dependency in &self.packages[name].dependencies {
                if dependency.compiled()
                    && dependency.name == "ffmpeg-next"
                    && on.contains(dependency.alias())
                {
                    if dependency.target.is_some() {
                        return Err("conditional FFmpeg dependency requires an explicit compiler input policy".into());
                    }
                    ffmpeg = true;
                }
            }
        }
        if ffmpeg {
            result.insert("FFMPEG_DIR".into(), PathBuf::new());
            result.insert("LIBCLANG_PATH".into(), "usr/lib/llvm-18/lib".into());
        }
        for (package, feature, environment, suffix) in [
            ("openpilot-encoderd", "native", "ENCODER_LIBYUV_LIB", "lib"),
            (
                "openpilot-startup-ui",
                "native",
                "STARTUP_UI_RAYLIB_ROOT",
                "",
            ),
            (
                "openpilot-webcam",
                "native",
                "OPENPILOT_WEBCAM_OPENCV_ROOT",
                "",
            ),
            (
                "openpilot-opencv-runtime",
                "native-skip-miri",
                "OPENPILOT_OPENCV_ROOT",
                "",
            ),
            (
                "openpilot-locationd",
                "solver",
                "LOCATIOND_EIGEN_INCLUDE",
                "",
            ),
            ("openpilot-paramsd", "solver", "PARAMSD_EIGEN_INCLUDE", ""),
        ] {
            if has(package, feature) {
                result.insert(environment.into(), suffix.into());
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn dependency(name: &str, alias: &str, optional: bool, defaults: bool) -> Value {
        json!({"name":name,"rename":alias,"kind":null,"path":format!("/owned/{name}"),
            "target":null,"optional":optional,"uses_default_features":defaults,"features":[]})
    }
    fn package(name: &str, features: Value, dependencies: Vec<Value>) -> Value {
        json!({"id":name,"name":name,"features":features,"dependencies":dependencies})
    }
    fn workspace(packages: Vec<Value>) -> Workspace {
        let members: Vec<_> = packages.iter().map(|p| p["id"].clone()).collect();
        Workspace::read(&json!({"workspace_members":members,"packages":packages})).unwrap()
    }
    fn group(root: &str, features: &[&str]) -> Group {
        Group {
            packages: vec![root.into()],
            features: features.iter().map(|s| (*s).into()).collect(),
            binaries: vec![],
            compiler_inputs: BTreeMap::new(),
        }
    }
    #[test]
    fn transitive_default_and_renamed_dependency_require_raylib_only_when_native_is_enabled() {
        for defaults in [false, true] {
            let graph = workspace(vec![
                package(
                    "ui",
                    json!({}),
                    vec![dependency(
                        "openpilot-startup-ui",
                        "visual",
                        false,
                        defaults,
                    )],
                ),
                package(
                    "openpilot-startup-ui",
                    json!({"default":["native"],"native":[]}),
                    vec![],
                ),
            ]);
            let inputs = graph.requirements(&group("ui", &[])).unwrap();
            assert_eq!(inputs.contains_key("STARTUP_UI_RAYLIB_ROOT"), defaults);
        }
    }
    #[test]
    fn weak_forwarding_does_not_activate_optional_dependency_and_is_revisited_after_activation() {
        for activate in [false, true] {
            let aliases = if activate {
                json!(["visual?/native", "dep:visual"])
            } else {
                json!(["visual?/native"])
            };
            let graph = workspace(vec![
                package(
                    "ui",
                    json!({"render":aliases}),
                    vec![dependency("openpilot-startup-ui", "visual", true, false)],
                ),
                package("openpilot-startup-ui", json!({"native":[]}), vec![]),
            ]);
            let inputs = graph.requirements(&group("ui", &["ui/render"])).unwrap();
            assert_eq!(inputs.contains_key("STARTUP_UI_RAYLIB_ROOT"), activate);
        }
    }
    #[test]
    fn transitive_logger_ffmpeg_is_required_but_dev_only_or_inactive_optional_edges_are_not() {
        for kind in [0, 1, 2, 3] {
            let mut root = dependency("logger", "logs", false, false);
            if kind == 1 {
                root["kind"] = "dev".into();
            }
            let ffmpeg = json!({"name":"ffmpeg-next","rename":null,"kind":null,"path":null,
                "target":null,"optional":kind>=2,"uses_default_features":false,"features":[]});
            let graph = workspace(vec![
                package("boot", json!({"record":["logs/codec"]}), vec![root]),
                package("logger", json!({"codec":["dep:ffmpeg-next"]}), vec![ffmpeg]),
            ]);
            let flags = if kind == 3 {
                vec!["boot/record"]
            } else {
                vec![]
            };
            let inputs = graph.requirements(&group("boot", &flags)).unwrap();
            let expected = kind == 0 || kind == 3;
            assert_eq!(inputs.contains_key("FFMPEG_DIR"), expected);
            assert_eq!(inputs.contains_key("LIBCLANG_PATH"), expected);
        }
    }
    #[test]
    fn compiled_build_dependency_features_are_followed_and_unknown_target_conditions_fail() {
        let mut dep = dependency("openpilot-opencv-runtime", "cv", false, false);
        dep["kind"] = "build".into();
        dep["features"] = json!(["native-skip-miri"]);
        for conditional in [false, true] {
            let mut dep = dep.clone();
            if conditional {
                dep["target"] = "cfg(unix)".into();
            }
            let graph = workspace(vec![
                package("tool", json!({}), vec![dep]),
                package(
                    "openpilot-opencv-runtime",
                    json!({"native-skip-miri":[]}),
                    vec![],
                ),
            ]);
            let result = graph.requirements(&group("tool", &[]));
            if conditional {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("conditional local"));
            } else {
                assert!(result.unwrap().contains_key("OPENPILOT_OPENCV_ROOT"));
            }
        }
    }
    #[test]
    fn build_and_runtime_instances_are_not_silently_feature_unified() {
        let runtime = dependency("openpilot-opencv-runtime", "cv", false, false);
        let mut build = runtime.clone();
        build["kind"] = "build".into();
        build["features"] = json!(["native-skip-miri"]);
        let graph = workspace(vec![
            package("tool", json!({}), vec![runtime, build]),
            package(
                "openpilot-opencv-runtime",
                json!({"native-skip-miri":[]}),
                vec![],
            ),
        ]);
        assert!(graph
            .requirements(&group("tool", &[]))
            .unwrap_err()
            .to_string()
            .contains("mixed local build/runtime"));
    }
}
