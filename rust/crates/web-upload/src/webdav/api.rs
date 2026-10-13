use super::{transport::component, WebDav};
use crate::{Error, Fields, Observer, Response, Value};
use std::path::Path;

/// Selected local artifacts for one route segment.
pub struct LocalFiles<'a> {
    pub folder: &'a Path,
    pub names: &'a [String],
}
/// Storage directory and segment beneath the configured share's routes collection.
pub struct RouteSegment<'a> {
    pub directory: &'a str,
    pub name: &'a str,
}

impl WebDav {
    /// Stores selected route files with bounded streaming and atomic MOVE publication.
    ///
    /// # Errors
    /// Rejects unsafe paths and returns upload, cancellation or partial cleanup failures.
    pub fn upload_folder(
        &self,
        source: LocalFiles<'_>,
        destination: RouteSegment<'_>,
        observer: &mut Observer<'_>,
    ) -> Result<bool, Error> {
        let folder = source.folder;
        let filenames = source.names;
        let directory = destination.directory;
        let segment = destination.name;
        let parts = ["routes", directory, segment];
        self.url(&parts, true)?;
        for filename in filenames {
            component(filename)?;
        }
        observer.check()?;
        self.collections(&parts, observer)?;
        for filename in filenames {
            self.put(
                &["routes", directory, segment, filename],
                &folder.join(filename),
                observer,
            )?;
        }
        observer.check()?;
        Ok(true)
    }

    /// Stores a tmux capture and optional settings under the branch and car/device directory.
    ///
    /// # Errors
    /// Returns configuration, file, transport or status failures without private data.
    pub fn upload_tmux(
        &self,
        metadata: &Fields,
        tmux: &Path,
        settings: Option<&Path>,
    ) -> Result<Response, Error> {
        fn text<'a>(fields: &'a Fields, key: &str, fallback: &'a str) -> &'a str {
            fields
                .get(key)
                .and_then(|v| match v {
                    Value::Text(s) if !s.is_empty() => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or(fallback)
        }
        let branch = text(metadata, "git_branch", "unknown").replace('/', "__");
        let device = crate::device_id(metadata)?;
        let directory = format!("{} {device}", text(metadata, "car_name", "none"));
        component(&branch)?;
        component(&directory)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let log = format!("tmux-{stamp}.log");
        let toggles = format!("toggles-{stamp}.json");
        let mut observer = Observer::default();
        self.collections(&[&branch, &directory], &mut observer)?;
        self.put(&[&branch, &directory, &log], tmux, &mut observer)?;
        let mut paths = vec![format!("{branch}/{directory}/{log}")];
        if let Some(settings) = settings {
            self.put(&[&branch, &directory, &toggles], settings, &mut observer)?;
            paths.push(format!("{branch}/{directory}/{toggles}"));
        }
        Ok(Response {
            status: 201,
            headers: ureq::http::HeaderMap::new(),
            body: serde_json::json!({"ok":true,"target":"webdav","storageOnly":true,"paths":paths})
                .to_string()
                .into_bytes(),
        })
    }
}
