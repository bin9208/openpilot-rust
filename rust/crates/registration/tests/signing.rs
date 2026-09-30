use openpilot_uploader::http::SigningKey;
#[test]
fn registration_claims_use_the_same_key_validation_as_upload_tokens() {
    let key = SigningKey::from_pem(jsonwebtoken::Algorithm::RS256, b"invalid".to_vec());
    assert!(key
        .token_claims(&serde_json::json!({"register":true,"exp":3600}))
        .is_err());
    assert!(key.token("identity", 0).is_err());
}
