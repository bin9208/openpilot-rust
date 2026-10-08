pub(crate) mod brotli;
pub(crate) mod compression;
mod files;
mod html;
mod index;
mod manifest;
pub(crate) mod mime;
mod request;

use crate::{config::Config, http::Body, http_response::text, static_assets::Assets, Error, Value};
use hyper::{header, Request, Response, StatusCode};
use std::sync::Arc;

pub struct StaticWeb {
    config: Config,
    assets: Assets,
    manifest: manifest::ManifestLoader,
}

impl StaticWeb {
    pub fn validate(&self) -> Result<(), Error> {
        self.config.validate()?;
        match std::fs::canonicalize(&self.config.shared_assets) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error::Source(
                format!("'{}' does not exist", self.config.shared_assets.display()),
            )),
            Err(error) => Err(error.into()),
            Ok(root) if !root.is_dir() => Err(Error::Source(format!(
                "'{}' is not a directory",
                self.config.shared_assets.display()
            ))),
            Ok(_) => Ok(()),
        }
    }

    pub fn new(config: Config) -> Arc<Self> {
        Arc::new(Self {
            config,
            assets: Assets::default(),
            manifest: manifest::ManifestLoader::default(),
        })
    }

    pub fn start_precompress(self: &Arc<Self>) -> tokio::task::JoinHandle<Result<(), Error>> {
        let web = Arc::clone(self);
        tokio::task::spawn_blocking(move || web.assets.precompress(&web.config.web))
    }

    pub async fn handle<T>(
        self: &Arc<Self>,
        request: &Request<T>,
        bootstrap: Option<Value>,
    ) -> Response<Body> {
        self.handle_with_bootstrap(request, move || {
            bootstrap
                .ok_or_else(|| Error::Source("index bootstrap dependency is unavailable".into()))
        })
        .await
    }

    /// Builds bootstrap after the index loads, preserving source recovery ordering.
    /// File responses, method rejection and index recovery skip the callback.
    pub async fn handle_with_bootstrap<T, F>(
        self: &Arc<Self>,
        request: &Request<T>,
        bootstrap: F,
    ) -> Response<Body>
    where
        F: FnOnce() -> Result<Value, Error> + Send + 'static,
    {
        let request = request::FileRequest::from_request(request);
        if !request.get && !request.head {
            let mut result = text(
                StatusCode::METHOD_NOT_ALLOWED,
                "405: Method Not Allowed",
                request.head,
            );
            result
                .headers_mut()
                .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
            return result;
        }
        let head = request.head;
        let result = if request.path == "/" {
            index::handle(self, bootstrap, head).await
        } else {
            let web = Arc::clone(self);
            match tokio::task::spawn_blocking(move || {
                if ["/js/", "/css/", "/assets/"]
                    .iter()
                    .any(|prefix| request.path.starts_with(prefix))
                {
                    web.assets.refresh(&web.config.web, &request.path)?;
                }
                let mut result = files::handle(&web.config, &request)?;
                if web
                    .assets
                    .immutable(&web.config.web, &request.path, request.query.as_deref())
                    && !result.headers().contains_key(header::CACHE_CONTROL)
                {
                    result.headers_mut().insert(
                        header::CACHE_CONTROL,
                        header::HeaderValue::from_static("public, max-age=31536000, immutable"),
                    );
                }
                Ok(result)
            })
            .await
            {
                Ok(result) => result,
                Err(error) => Err(Error::Source(error.to_string())),
            }
        };
        result.unwrap_or_else(|error| {
            eprintln!("static request: {error}");
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                head,
            )
        })
    }
}
