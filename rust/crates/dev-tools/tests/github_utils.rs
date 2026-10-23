//! Inert source tests: synthetic strings/files and an injected transport only.
//! No network, auth discovery, real credentials, provider or external process.
use openpilot_dev_tools::github_utils::{Call, GithubUtils, Request, Response, Result, Transport};
use openpilot_log_format::JsonValue;
use std::{collections::VecDeque, fs, path::PathBuf};

#[derive(Default)]
struct Fake {
    replies: VecDeque<std::result::Result<Response, &'static str>>,
    calls: Vec<Request>,
    rewrite_on_request: Option<(PathBuf, Vec<u8>)>,
}
impl Fake {
    fn new(replies: &[(u16, &str)]) -> Self {
        Self {
            replies: replies
                .iter()
                .map(|(status, body)| {
                    Ok(Response {
                        status: *status,
                        body: body.as_bytes().to_vec(),
                    })
                })
                .collect(),
            ..Self::default()
        }
    }
}
impl Transport for Fake {
    fn request(&mut self, request: Request) -> Result<Response> {
        self.calls.push(request);
        if let Some((path, bytes)) = self.rewrite_on_request.take() {
            // Truncate the existing inode, not unlink it. An already-open but
            // unread descriptor would observe these replacement bytes.
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)?;
            file.write_all(&bytes)?;
        }
        self.replies
            .pop_front()
            .expect("unexpected extra request")
            .map_err(Into::into)
    }
}
fn client(replies: &[(u16, &str)]) -> GithubUtils<Fake> {
    GithubUtils::new(Fake::new(replies), "synthetic-api", "synthetic-data")
}
fn number(value: &JsonValue) -> String {
    value.to_json().unwrap()
}
fn upload_body(bucket: &str, name: &str, sha: &str, content: &str) -> String {
    // Source AST constant fragments include exactly21 spaces at each continuation.
    let spaces = " ".repeat(21);
    format!("{{\"message\":\"uploading {name}\",{spaces}\"branch\":\"{bucket}\",{spaces}\"committer\":{{\"name\":\"Vehicle Researcher\", \"email\": \"user@comma.ai\"}},{spaces}{sha}{spaces}\"content\":\"{content}\"}}")
}

#[test]
fn routes_tokens_accept_and_defaults_are_exact_without_token_discovery() {
    let mut g = client(&[(200, "not-json"), (302, "not-json"), (200, "")]);
    assert_eq!(
        g.api_route(),
        "https://api.github.com/repos/commaai/openpilot"
    );
    assert_eq!(
        g.data_route(),
        "https://api.github.com/repos/commaai/ci-artifacts"
    );
    g.owner = "owner/with space".into();
    g.api_repo = "api/".into();
    g.data_repo = "data?".into();
    let r = g.api_call("/literal?x=1", Call::default()).unwrap();
    assert_eq!(r.body, b"not-json"); // api_call must not eagerly decode JSON.
    let a = &g.transport.calls[0];
    assert_eq!(
        a.url,
        "https://api.github.com/repos/owner/with space/api///literal?x=1"
    );
    assert_eq!(a.method, "GET");
    assert_eq!(a.data, "");
    assert_eq!(
        a.headers,
        [
            (
                "Authorization".to_owned(),
                "Bearer synthetic-api".to_owned()
            ),
            (
                "Accept".to_owned(),
                "application/vnd.github+json".to_owned()
            )
        ]
    );
    g.api_call(
        "x",
        Call {
            data_call: true,
            method: "PATCH",
            data: "raw\n\"",
            accept: ".raw",
            ..Call::default()
        },
    )
    .unwrap();
    assert_eq!(
        g.transport.calls[1],
        Request {
            method: "PATCH".into(),
            url: "https://api.github.com/repos/owner/with space/data?/x".into(),
            headers: vec![
                ("Authorization".into(), "Bearer synthetic-data".into()),
                ("Accept".into(), "application/vnd.github.raw+json".into())
            ],
            data: "raw\n\"".into()
        }
    );
    g.data_token.clear();
    g.api_call(
        "x",
        Call {
            data_call: true,
            accept: ".raw",
            ..Call::default()
        },
    )
    .unwrap();
    assert!(g.transport.calls[2].headers.is_empty()); // Accept also absent.
}

#[test]
fn http_ok_includes_redirects_and_failure_does_not_decode_body() {
    let mut g = client(&[(399, "bad"), (400, "bad"), (503, "bad")]);
    assert_eq!(g.api_call("x", Call::default()).unwrap().status, 399);
    assert_eq!(
        g.api_call("x", Call::default()).err().unwrap().to_string(),
        "Call to https://api.github.com/repos/commaai/openpilot/x failed with 400"
    );
    assert_eq!(
        g.api_call(
            "x",
            Call {
                raise_on_failure: false,
                ..Call::default()
            }
        )
        .unwrap()
        .status,
        503
    );
    assert_eq!(g.transport.calls.len(), 3);
}

#[test]
fn selected_token_is_independent_and_empty_api_omits_both_headers() {
    let mut g = client(&[(200, ""), (200, "")]);
    g.api_token.clear();
    g.api_call("x", Call::default()).unwrap();
    g.api_call(
        "x",
        Call {
            data_call: true,
            ..Call::default()
        },
    )
    .unwrap();
    assert!(g.transport.calls[0].headers.is_empty());
    assert_eq!(g.transport.calls[1].headers[0].1, "Bearer synthetic-data");
}

#[test]
fn lookup_json_fields_null_and_status_failure_are_not_normalized() {
    let mut g = client(&[
        (302, r#"{"object":{"sha":"bucket"}}"#),
        (404, "invalid"),
        (200, r#"{"sha":null}"#),
        (200, r#"{"download_url":false}"#),
        (200, r#"[{"number":7}]"#),
    ]);
    assert_eq!(
        g.get_bucket_sha("a/b").unwrap().unwrap().to_utf8().unwrap(),
        "bucket"
    );
    assert!(g.get_file_sha("bucket?", "dir/name").unwrap().is_none());
    assert_eq!(number(&g.get_file_sha("b", "f").unwrap().unwrap()), "null");
    assert_eq!(number(&g.get_file_url("b", "f").unwrap()), "false");
    assert_eq!(number(&g.get_pr_number("branch/slash").unwrap()), "7");
    assert_eq!(
        g.transport.calls[0].url,
        "https://api.github.com/repos/commaai/ci-artifacts/git/ref/heads/a/b"
    );
    assert!(g.transport.calls[1]
        .url
        .ends_with("contents/dir/name?ref=bucket?"));
    assert!(g.transport.calls[4]
        .url
        .ends_with("commits/branch/slash/pulls"));
}

#[test]
fn missing_or_wrong_json_fields_and_empty_pr_stop_at_the_access() {
    for body in ["[]", "[{}]", "null", "{}", "bad-json"] {
        let mut g = client(&[(200, body)]);
        assert!(g.get_pr_number("x").is_err());
        assert_eq!(g.transport.calls.len(), 1);
    }
    for body in ["{}", r#"{"object":null}"#, r#"{"object":{}}"#] {
        assert!(client(&[(200, body)]).get_bucket_sha("x").is_err());
    }
    assert!(client(&[(200, "{}")]).get_file_sha("x", "f").is_err());
    assert!(client(&[(404, "bad")]).get_file_url("x", "f").is_err());
}

#[test]
fn bucket_existing_truthy_stops_and_missing_master_interpolates_none() {
    let mut g = client(&[(200, r#"{"object":{"sha":"existing"}}"#)]);
    g.create_bucket("branch").unwrap();
    assert_eq!(g.transport.calls.len(), 1);
    let mut g = client(&[(404, "bad"), (404, "bad"), (201, "bad")]);
    g.create_bucket("branch/\"").unwrap();
    assert_eq!(
        g.transport
            .calls
            .iter()
            .map(|r| r.method.as_str())
            .collect::<Vec<_>>(),
        ["GET", "GET", "POST"]
    );
    assert!(g.transport.calls[1].url.ends_with("git/ref/heads/master"));
    assert_eq!(
        g.transport.calls[2].data,
        "{\"ref\":\"refs/heads/branch/\"\", \"sha\":\"None\"}"
    );
    assert!(g.transport.calls[2].url.ends_with("/ci-artifacts/git/refs"));
}

#[test]
fn bucket_false_sha_still_queries_master_and_retains_raw_interpolation() {
    let mut g = client(&[
        (200, r#"{"object":{"sha":0}}"#),
        (200, r#"{"object":{"sha":"a\"b"}}"#),
        (201, ""),
    ]);
    g.create_bucket("new").unwrap();
    assert_eq!(
        g.transport.calls[2].data,
        "{\"ref\":\"refs/heads/new\", \"sha\":\"a\"b\"}"
    );
}

#[test]
fn upload_reads_file_before_lookup_then_puts_exact_base64_and_raw_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.bin");
    fs::write(&path, [0, 255, 1]).unwrap();
    let mut g = client(&[(200, r#"{"sha":"old\"sha"}"#), (200, "not-json")]);
    // Rewriting/truncating the same file at GET differentiates read-before
    // from open-before-GET followed by a delayed read through the same inode.
    g.transport.rewrite_on_request = Some((path.clone(), b"changed-at-lookup".to_vec()));
    g.upload_file("bucket/\"", &path, "f\"\n.bin").unwrap();
    assert_eq!(g.transport.calls.len(), 2);
    assert_eq!(g.transport.calls[0].method, "GET");
    assert_eq!(g.transport.calls[1].method, "PUT");
    assert_eq!(
        g.transport.calls[1].data.as_bytes(),
        upload_body("bucket/\"", "f\"\n.bin", "\"sha\":\"old\"sha\",", "AP8B").as_bytes()
    );
    assert!(g.transport.calls[1].url.ends_with("/contents/f\"\n.bin"));
    assert_eq!(fs::read(&path).unwrap(), b"changed-at-lookup");
}

#[test]
fn upload_file_missing_and_lookup_error_prevent_later_requests() {
    let dir = tempfile::tempdir().unwrap();
    let mut g = client(&[]);
    assert!(g
        .upload_file("b", &dir.path().join("missing"), "x")
        .is_err());
    assert!(g.transport.calls.is_empty());
    let path = dir.path().join("empty");
    fs::write(&path, []).unwrap();
    let mut g = client(&[(200, "{}")]);
    assert!(g.upload_file("b", &path, "x").is_err());
    assert_eq!(g.transport.calls.len(), 1);
}

#[test]
fn upload_false_or_absent_sha_omits_sha_and_empty_bytes_stay_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty");
    fs::write(&path, []).unwrap();
    for (status, body) in [
        (404, "bad"),
        (200, r#"{"sha":false}"#),
        (200, r#"{"sha":""}"#),
        (200, r#"{"sha":0}"#),
        (200, r#"{"sha":null}"#),
    ] {
        let mut g = client(&[(status, body), (201, "bad")]);
        g.upload_file("b", &path, "f").unwrap();
        assert_eq!(g.transport.calls[1].data, upload_body("b", "f", "", ""));
    }
}

#[test]
fn upload_files_creates_bucket_first_and_stops_in_supplied_order() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    fs::write(&a, b"a").unwrap();
    let files = vec![
        ("first".into(), a),
        ("second".into(), dir.path().join("missing")),
    ];
    let mut g = client(&[
        (200, r#"{"object":{"sha":"yes"}}"#),
        (404, "bad"),
        (201, ""),
    ]);
    assert!(g.upload_files("b", &files).is_err());
    assert_eq!(g.transport.calls.len(), 3);
    assert!(g.transport.calls[0].url.ends_with("git/ref/heads/b"));
    assert!(g.transport.calls[1].url.ends_with("contents/first?ref=b"));
    assert!(g.transport.calls[2].url.ends_with("contents/first"));
    let mut g = client(&[(200, r#"{"object":{"sha":"yes"}}"#)]);
    g.upload_files("b", &[]).unwrap();
    assert_eq!(g.transport.calls.len(), 1);
}

#[test]
fn transport_error_propagates_without_retry() {
    let mut g = client(&[]);
    g.transport.replies.push_back(Err("synthetic-failure"));
    assert_eq!(
        g.create_bucket("x").err().unwrap().to_string(),
        "synthetic-failure"
    );
    assert_eq!(g.transport.calls.len(), 1);
}

#[test]
fn comment_default_posts_unescaped_body_and_pr_first_number() {
    let mut g = client(&[(200, r#"[{"number":8},{"number":9}]"#), (201, "invalid")]);
    g.comment_on_pr("a\"\n\\b", "branch", "", false).unwrap();
    assert_eq!(g.transport.calls.len(), 2);
    assert!(g.transport.calls[1].url.ends_with("issues/8/comments"));
    assert_eq!(g.transport.calls[1].method, "POST");
    assert_eq!(g.transport.calls[1].data, "{\"body\": \"a\"\n\\b\"}");
}

#[test]
fn overwrite_patches_first_matching_id_only_after_scanning_all_rows() {
    let mut g = client(&[
        (200, r#"[{"number":5}]"#),
        (
            200,
            r#"[{"id":12,"user":{"login":"me"}},{"id":13,"user":{"login":"me"}},{"user":{"login":"other"}}]"#,
        ),
        (200, ""),
    ]);
    g.comment_on_pr("body", "branch", "me", true).unwrap();
    assert_eq!(g.transport.calls.len(), 3);
    assert_eq!(g.transport.calls[2].method, "PATCH");
    assert!(g.transport.calls[2].url.ends_with("issues/comments/12"));
    let mut g = client(&[
        (200, r#"[{"number":5}]"#),
        (200, r#"[{"id":12,"user":{"login":"me"}},{"user":{}}]"#),
    ]);
    assert!(g.comment_on_pr("body", "branch", "me", true).is_err());
    assert_eq!(g.transport.calls.len(), 2); // Must not PATCH early after first match.
}

#[test]
fn overwrite_no_match_posts_and_nonmatching_missing_id_is_not_read() {
    let mut g = client(&[
        (200, r#"[{"number":null}]"#),
        (
            200,
            r#"[{"user":{"login":"other"}},{"user":{"login":null}}]"#,
        ),
        (201, ""),
    ]);
    g.comment_on_pr("body", "branch", "me", true).unwrap();
    assert!(g.transport.calls[1].url.ends_with("issues/None/comments"));
    assert_eq!(g.transport.calls[2].method, "POST");
    let mut g = client(&[
        (200, r#"[{"number":5}]"#),
        (200, r#"[{"user":{"login":"me"}}]"#),
    ]);
    assert!(g.comment_on_pr("body", "branch", "me", true).is_err());
    assert_eq!(g.transport.calls.len(), 2);
}

#[test]
fn images_513_upload_prefix_requested_branch_odd_rows_and_default_post() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image");
    fs::write(&path, b"x").unwrap();
    for n in [0, 1, 2, 3] {
        let mut replies = vec![(200, r#"{"object":{"sha":"yes"}}"#)];
        for _ in 0..n {
            replies.extend([(404, "bad"), (201, "")]);
        }
        replies.extend([(200, r#"[{"number":42}]"#), (201, "")]);
        let mut g = client(&replies);
        let images = (0..n)
            .map(|i| (format!("image{i}.png"), path.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            g.get_bucket_link("bucket"),
            "https://raw.githubusercontent.com/commaai/ci-artifacts/refs/heads/bucket"
        );
        g.comment_images_on_pr("<title>", "commenter-513", "branch-513", "bucket", &images)
            .unwrap();
        let calls = &g.transport.calls;
        assert_eq!(calls.len(), 1 + 2 * n + 2);
        assert_eq!(calls[0].method, "GET");
        assert!(calls[0].url.ends_with("git/ref/heads/bucket"));
        for i in 0..n {
            assert_eq!(calls[1 + 2 * i].method, "GET");
            assert!(calls[1 + 2 * i]
                .url
                .ends_with(&format!("contents/image{i}.png?ref=bucket")));
            assert_eq!(calls[2 + 2 * i].method, "PUT");
            assert!(calls[2 + 2 * i]
                .url
                .ends_with(&format!("contents/image{i}.png")));
        }
        assert_eq!(calls[calls.len() - 2].method, "GET");
        assert!(calls[calls.len() - 2]
            .url
            .ends_with("commits/branch-513/pulls"));
        assert!(calls.last().unwrap().url.ends_with("issues/42/comments"));
        let mut table = String::from("<details><summary><title></summary><table>");
        for i in 0..n {
            if i % 2 == 0 {
                table.push_str("<tr>");
            }
            table.push_str(&format!("<td><img src=\\\"https://raw.githubusercontent.com/commaai/ci-artifacts/bucket/image{i}.png\\\"></td>"));
            if i % 2 == 1 {
                table.push_str("</tr>");
            }
        }
        table.push_str("</table></details>");
        assert_eq!(
            calls.last().unwrap().data,
            format!("{{\"body\": \"{table}\"}}")
        );
        assert_eq!(calls.last().unwrap().method, "POST");
    }
}

#[test]
fn images_513_missing_private_file_stops_before_pr_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let mut g = client(&[(200, r#"{"object":{"sha":"yes"}}"#)]);
    assert!(g
        .comment_images_on_pr(
            "t",
            "commenter-513",
            "branch-513",
            "b",
            &[("f".into(), dir.path().join("missing"))]
        )
        .is_err());
    assert_eq!(g.transport.calls.len(), 1);
    assert_eq!(g.transport.calls[0].method, "GET");
    assert!(g.transport.calls[0].url.ends_with("git/ref/heads/b"));
}

fn images_513_assert_upload_prefix(calls: &[Request]) {
    assert!(calls.len() >= 3);
    assert_eq!(calls[0].method, "GET");
    assert!(calls[0].url.ends_with("git/ref/heads/bucket"));
    assert_eq!(calls[1].method, "GET");
    assert!(calls[1].url.ends_with("contents/image.png?ref=bucket"));
    assert_eq!(calls[2].method, "PUT");
    assert!(calls[2].url.ends_with("contents/image.png"));
    assert_eq!(
        calls[2].data,
        upload_body("bucket", "image.png", "", "AP8B")
    );
}

#[test]
fn images_513_pr_lookup_failures_retain_complete_upload_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private-image");
    fs::write(&path, [0, 255, 1]).unwrap();
    for (status, body) in [
        (503, "not-json"),
        (200, "not-json"),
        (200, "[]"),
        (200, "{}"),
        (200, "\"\""),
        (200, "null"),
        (200, "[{}]"),
        (200, "[null]"),
    ] {
        let mut g = client(&[
            (200, r#"{"object":{"sha":"exists"}}"#),
            (404, "not-json"),
            (201, "not-json"),
            (status, body),
        ]);
        assert!(
            g.comment_images_on_pr(
                "title",
                "commenter-513",
                "branch-513",
                "bucket",
                &[("image.png".into(), path.clone())]
            )
            .is_err(),
            "{status}: {body}"
        );
        let calls = &g.transport.calls;
        assert_eq!(calls.len(), 4);
        images_513_assert_upload_prefix(calls);
        assert_eq!(calls[3].method, "GET");
        assert!(calls[3].url.ends_with("commits/branch-513/pulls"));
    }
}

#[test]
fn images_513_put_and_comment_failures_keep_existing_effect_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private-image");
    fs::write(&path, [0, 255, 1]).unwrap();
    for fail_at_put in [true, false] {
        let mut replies = vec![(200, r#"{"object":{"sha":"exists"}}"#), (404, "not-json")];
        if fail_at_put {
            replies.push((503, "not-json"));
        } else {
            replies.extend([
                (201, "not-json"),
                (200, r#"[{"number":42}]"#),
                (503, "not-json"),
            ]);
        }
        let mut g = client(&replies);
        let error = g
            .comment_images_on_pr(
                "title",
                "commenter-513",
                "branch-513",
                "bucket",
                &[("image.png".into(), path.clone())],
            )
            .unwrap_err()
            .to_string();
        assert!(error.ends_with("failed with 503"));
        let calls = &g.transport.calls;
        images_513_assert_upload_prefix(calls);
        assert_eq!(calls.len(), if fail_at_put { 3 } else { 5 });
        if !fail_at_put {
            assert_eq!(calls[3].method, "GET");
            assert!(calls[3].url.ends_with("commits/branch-513/pulls"));
            assert_eq!(calls[4].method, "POST");
            assert!(calls[4].url.ends_with("issues/42/comments"));
            assert_eq!(calls[4].data, "{\"body\": \"<details><summary>title</summary><table><tr><td><img src=\\\"https://raw.githubusercontent.com/commaai/ci-artifacts/bucket/image.png\\\"></td></table></details>\"}");
        }
    }
}

#[test]
fn images_513_empty_commenter_or_branch_keep_false_overwrite_default() {
    for (commenter, branch) in [("", "branch-513"), ("commenter-513", ""), ("", "")] {
        let mut g = client(&[
            (200, r#"{"object":{"sha":"exists"}}"#),
            (200, r#"[{"number":42}]"#),
            (201, "not-json"),
        ]);
        g.comment_images_on_pr("raw\"title\n", commenter, branch, "bucket", &[])
            .unwrap();
        let calls = &g.transport.calls;
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].method, "GET");
        assert!(calls[0].url.ends_with("git/ref/heads/bucket"));
        assert_eq!(calls[1].method, "GET");
        assert!(calls[1].url.ends_with(&format!("commits/{branch}/pulls")));
        assert_eq!(calls[2].method, "POST");
        assert!(calls[2].url.ends_with("issues/42/comments"));
        assert_eq!(
            calls[2].data,
            "{\"body\": \"<details><summary>raw\"title\n</summary><table></table></details>\"}"
        );
    }
}

#[test]
fn truthy_returned_json_values_use_existing_python_str_not_json_serialization() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input");
    fs::write(&path, b"f").unwrap();
    for (json, spelling) in [
        ("true", "True"),
        ("7", "7"),
        (r#"["x",false]"#, "['x', False]"),
    ] {
        let lookup = format!("{{\"sha\":{json}}}");
        let mut g = client(&[(200, &lookup), (201, "")]);
        g.upload_file("b", &path, "f").unwrap();
        assert_eq!(
            g.transport.calls[1].data,
            upload_body("b", "f", &format!("\"sha\":\"{spelling}\","), "Zg==")
        );
    }
}

#[test]
fn failed_bucket_creation_or_first_put_stops_before_reading_next_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("first");
    fs::write(&path, b"f").unwrap();
    let files = vec![
        ("first".into(), path),
        ("second".into(), dir.path().join("missing")),
    ];
    let mut g = client(&[
        (404, ""),
        (200, r#"{"object":{"sha":"master"}}"#),
        (403, "bad"),
    ]);
    let error = g.upload_files("b", &files).err().unwrap().to_string();
    assert!(error.ends_with("failed with 403"));
    assert_eq!(g.transport.calls.len(), 3);
    assert_eq!(g.transport.calls[2].method, "POST");
    let mut g = client(&[
        (200, r#"{"object":{"sha":"yes"}}"#),
        (404, ""),
        (500, "bad"),
    ]);
    let error = g.upload_files("b", &files).err().unwrap().to_string();
    assert!(error.ends_with("failed with 500"));
    assert_eq!(g.transport.calls.len(), 3);
    assert_eq!(g.transport.calls[2].method, "PUT");
}

#[test]
fn pr_failure_and_comment_list_failure_prevent_any_write_request() {
    let mut g = client(&[(200, "[]")]);
    assert!(g.comment_on_pr("body", "branch", "me", true).is_err());
    assert_eq!(g.transport.calls.len(), 1);
    let mut g = client(&[(200, r#"[{"number":4}]"#), (403, "bad")]);
    assert!(g.comment_on_pr("body", "branch", "me", true).is_err());
    assert_eq!(g.transport.calls.len(), 2);
    assert!(g.transport.calls.iter().all(|r| r.method == "GET"));
}

#[test]
fn overwrite_empty_object_text_and_array_post_after_zero_iterations() {
    for body in ["{}", "\"\"", "[]"] {
        let mut g = client(&[(200, r#"[{"number":5}]"#), (200, body), (201, "not-json")]);
        g.comment_on_pr("literal\"body", "branch", "me", true)
            .unwrap();
        assert_eq!(g.transport.calls.len(), 3);
        assert!(g.transport.calls[0].url.ends_with("commits/branch/pulls"));
        assert_eq!(g.transport.calls[1].method, "GET");
        assert!(g.transport.calls[1].url.ends_with("issues/5/comments"));
        assert_eq!(g.transport.calls[2].method, "POST");
        assert_eq!(g.transport.calls[2].url, g.transport.calls[1].url);
        assert_eq!(g.transport.calls[2].data, "{\"body\": \"literal\"body\"}");
    }
}

#[test]
fn overwrite_nonempty_wrong_rows_and_noniterable_json_still_fail_without_write() {
    for body in [
        r#"{"a":0}"#,
        r#""a""#,
        "null",
        "false",
        "0",
        "1.5",
        "[null]",
        "[0]",
        r#"["a"]"#,
        "[{}]",
        r#"[{"user":{}}]"#,
        r#"[{"user":{"login":"me"}}]"#,
    ] {
        let mut g = client(&[(200, r#"[{"number":5}]"#), (200, body)]);
        assert!(
            g.comment_on_pr("body", "branch", "me", true).is_err(),
            "{body}"
        );
        assert_eq!(g.transport.calls.len(), 2, "{body}");
        assert!(
            g.transport.calls.iter().all(|call| call.method == "GET"),
            "{body}"
        );
    }
}

#[test]
fn comment_zero_iteration_adapter_does_not_relax_pull_request_first_index() {
    for body in ["{}", "\"\"", "[]", "null"] {
        let mut g = client(&[(200, body)]);
        assert!(
            g.comment_on_pr("body", "branch", "me", true).is_err(),
            "{body}"
        );
        assert_eq!(g.transport.calls.len(), 1);
    }
}
