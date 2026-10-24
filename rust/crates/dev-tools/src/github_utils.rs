//! Injected-only port of openpilot/tools/lib/github_utils.py at d8767070c.
//! Copyright (c) 2018, Comma.ai, Inc.; repository MIT license applies.
//!
//! No HTTP adapter, token discovery, environment access, CLI or external action.
//! Caller-supplied transport owns all effects. JSON interpolation is deliberately
//! literal, including malformed-body quirks. Image PR routing is corrected by #513.
use base64::{engine::general_purpose::STANDARD, Engine};
use openpilot_log_format::{JsonValue, JsonView};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub url: String,
    /// Only the explicit source headers, in their insertion order. This model
    /// does not claim Requests' implicit wire headers/encoding/transport policy.
    pub headers: Vec<(String, String)>,
    pub data: String,
}
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    /// Supplied response content. json() is lazy; no Requests decoder claim.
    pub body: Vec<u8>,
}
impl Response {
    pub fn ok(&self) -> bool {
        self.status < 400
    }
    pub fn json(&self) -> Result<JsonValue> {
        Ok(JsonValue::parse(std::str::from_utf8(&self.body)?)?)
    }
}
pub trait Transport {
    fn request(&mut self, request: Request) -> Result<Response>;
}

#[derive(Clone, Copy)]
pub struct Call<'a> {
    pub data: &'a str,
    pub method: &'a str,
    pub accept: &'a str,
    pub data_call: bool,
    pub raise_on_failure: bool,
}
impl Default for Call<'_> {
    fn default() -> Self {
        Self {
            data: "",
            method: "GET",
            accept: "",
            data_call: false,
            raise_on_failure: true,
        }
    }
}

/// Mutable source attributes remain explicit caller data. Empty string tokens
/// mean absent headers, matching the project's string-token caller. Nonstring
/// Python constructor arguments are outside this bounded Rust interface.
pub struct GithubUtils<T> {
    pub owner: String,
    pub api_repo: String,
    pub data_repo: String,
    pub api_token: String,
    pub data_token: String,
    pub transport: T,
}
impl<T: Transport> GithubUtils<T> {
    pub fn new(transport: T, api_token: impl Into<String>, data_token: impl Into<String>) -> Self {
        Self {
            owner: "commaai".into(),
            api_repo: "openpilot".into(),
            data_repo: "ci-artifacts".into(),
            api_token: api_token.into(),
            data_token: data_token.into(),
            transport,
        }
    }
    pub fn api_route(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}",
            self.owner, self.api_repo
        )
    }
    pub fn data_route(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}",
            self.owner, self.data_repo
        )
    }
    pub fn api_call(&mut self, path: &str, call: Call<'_>) -> Result<Response> {
        let token = if call.data_call {
            &self.data_token
        } else {
            &self.api_token
        };
        let headers = if token.is_empty() {
            Vec::new()
        } else {
            vec![
                ("Authorization".into(), format!("Bearer {token}")),
                (
                    "Accept".into(),
                    format!("application/vnd.github{}+json", call.accept),
                ),
            ]
        };
        let route = if call.data_call {
            self.data_route()
        } else {
            self.api_route()
        };
        let url = format!("{route}/{path}");
        let response = self.transport.request(Request {
            method: call.method.into(),
            url: url.clone(),
            headers,
            data: call.data.into(),
        })?;
        if !response.ok() && call.raise_on_failure {
            return Err(format!("Call to {url} failed with {}", response.status).into());
        }
        Ok(response)
    }
    pub fn upload_file(&mut self, bucket: &str, path: &Path, file_name: &str) -> Result<()> {
        // Retain the opened file until after lookup/PUT, like the original with.
        let mut file = File::open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let encoded = STANDARD.encode(&bytes);
        let sha = self.get_file_sha(bucket, file_name)?;
        let sha = match sha.as_ref().filter(|value| truth(value)) {
            Some(value) => format!("\"sha\":\"{}\",", python_str(value)?),
            None => String::new(),
        };
        let data = format!(concat!(
            "{{\"message\":\"uploading {}\",",
            "                     \"branch\":\"{}\",",
            "                     \"committer\":{{\"name\":\"Vehicle Researcher\", \"email\": \"user@comma.ai\"}},",
            "                     {}",
            "                     \"content\":\"{}\"}}"
        ), file_name, bucket, sha, encoded);
        self.api_call(
            &format!("contents/{file_name}"),
            Call {
                data: &data,
                method: "PUT",
                data_call: true,
                ..Call::default()
            },
        )?;
        drop(file);
        Ok(())
    }
    pub fn upload_files(&mut self, bucket: &str, files: &[(String, PathBuf)]) -> Result<()> {
        self.create_bucket(bucket)?;
        for (file_name, path) in files {
            self.upload_file(bucket, path, file_name)?;
        }
        Ok(())
    }
    pub fn create_bucket(&mut self, bucket: &str) -> Result<()> {
        if self.get_bucket_sha(bucket)?.as_ref().is_some_and(truth) {
            return Ok(());
        }
        let master_sha = self.get_bucket_sha("master")?;
        let master_sha = match master_sha {
            Some(value) => python_str(&value)?,
            None => String::from("None"),
        };
        let data = format!("{{\"ref\":\"refs/heads/{bucket}\", \"sha\":\"{master_sha}\"}}");
        self.api_call(
            "git/refs",
            Call {
                data: &data,
                method: "POST",
                data_call: true,
                ..Call::default()
            },
        )?;
        Ok(())
    }
    pub fn get_bucket_sha(&mut self, bucket: &str) -> Result<Option<JsonValue>> {
        let response = self.api_call(
            &format!("git/ref/heads/{bucket}"),
            Call {
                data_call: true,
                raise_on_failure: false,
                ..Call::default()
            },
        )?;
        if response.ok() {
            Ok(Some(field(&field(&response.json()?, "object")?, "sha")?))
        } else {
            Ok(None)
        }
    }
    pub fn get_file_url(&mut self, bucket: &str, file_name: &str) -> Result<JsonValue> {
        let response = self.api_call(
            &format!("contents/{file_name}?ref={bucket}"),
            Call {
                data_call: true,
                ..Call::default()
            },
        )?;
        field(&response.json()?, "download_url")
    }
    pub fn get_file_sha(&mut self, bucket: &str, file_name: &str) -> Result<Option<JsonValue>> {
        let response = self.api_call(
            &format!("contents/{file_name}?ref={bucket}"),
            Call {
                data_call: true,
                raise_on_failure: false,
                ..Call::default()
            },
        )?;
        if response.ok() {
            Ok(Some(field(&response.json()?, "sha")?))
        } else {
            Ok(None)
        }
    }
    pub fn get_pr_number(&mut self, pr_branch: &str) -> Result<JsonValue> {
        let response = self.api_call(&format!("commits/{pr_branch}/pulls"), Call::default())?;
        let rows = array(&response.json()?)?;
        field(
            rows.first()
                .ok_or("pull request response index 0 missing")?,
            "number",
        )
    }
    pub fn get_bucket_link(&self, bucket: &str) -> String {
        format!(
            "https://raw.githubusercontent.com/{}/{}/refs/heads/{bucket}",
            self.owner, self.data_repo
        )
    }
    pub fn comment_on_pr(
        &mut self,
        comment: &str,
        pr_branch: &str,
        commenter: &str,
        overwrite: bool,
    ) -> Result<()> {
        let pr_number = python_str(&self.get_pr_number(pr_branch)?)?;
        let data = format!("{{\"body\": \"{comment}\"}}");
        let path = format!("issues/{pr_number}/comments");
        if overwrite {
            let response = self.api_call(&path, Call::default())?;
            let mut comments = Vec::new();
            // Complete the source list comprehension before selecting its first
            // element; a later malformed row must still fail before PATCH.
            for row in comment_rows(&response.json()?)? {
                if field(&field(&row, "user")?, "login")?.text_eq(commenter) {
                    comments.push(field(&row, "id")?);
                }
            }
            if let Some(id) = comments.first() {
                self.api_call(
                    &format!("issues/comments/{}", python_str(id)?),
                    Call {
                        data: &data,
                        method: "PATCH",
                        ..Call::default()
                    },
                )?;
                return Ok(());
            }
        }
        self.api_call(
            &path,
            Call {
                data: &data,
                method: "POST",
                ..Call::default()
            },
        )?;
        Ok(())
    }
    pub fn comment_images_on_pr(
        &mut self,
        title: &str,
        commenter: &str,
        pr_branch: &str,
        bucket: &str,
        images: &[(String, PathBuf)],
    ) -> Result<()> {
        self.upload_files(bucket, images)?;
        let mut table = format!("<details><summary>{title}</summary><table>");
        for (i, (name, _)) in images.iter().enumerate() {
            if i % 2 == 0 {
                table.push_str("<tr>");
            }
            table.push_str(&format!("<td><img src=\\\"https://raw.githubusercontent.com/{}/{}/{bucket}/{name}\\\"></td>", self.owner, self.data_repo));
            if i % 2 != 0 {
                table.push_str("</tr>");
            }
        }
        table.push_str("</table></details>");
        // #513: route the requested branch/commenter; retain overwrite=false.
        self.comment_on_pr(&table, pr_branch, commenter, false)
    }
}

fn field(value: &JsonValue, name: &str) -> Result<JsonValue> {
    value
        .get(name)
        .ok_or_else(|| format!("missing JSON field: {name}").into())
}
fn array(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(values) => Ok(values),
        _ => Err("JSON response must be a list".into()),
    }
}
// Python's comments comprehension iterates empty dict/text zero times.
// Nonempty dict/text yield keys/characters which cannot provide ["user"],
// so they still fail before any write. PR first-index lookup stays separate.
fn comment_rows(value: &JsonValue) -> Result<Vec<JsonValue>> {
    match value.view() {
        JsonView::Array(values) => Ok(values),
        JsonView::Object(values) if values.is_empty() => Ok(Vec::new()),
        JsonView::Text([]) => Ok(Vec::new()),
        _ => Err("comments response must be a list or empty dict/text".into()),
    }
}
fn python_str(value: &JsonValue) -> Result<String> {
    openpilot_runtime_version::python_str(value)?
        .into_iter()
        .map(|point| {
            char::from_u32(point)
                .ok_or_else(|| "non-UTF8 Python string rendering is not admitted".into())
        })
        .collect()
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value
            .trim_start_matches('-')
            .bytes()
            .any(|byte| byte != b'0'),
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
