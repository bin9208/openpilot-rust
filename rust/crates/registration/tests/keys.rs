use openpilot_registration::get_key_pair;
use std::fs;
#[test]
fn rsa_pair_precedes_ec_and_reads_universal_newlines() {
    let dir = tempfile::tempdir().unwrap();
    let comma = dir.path().join("comma");
    fs::create_dir(&comma).unwrap();
    for (name, data) in [
        ("id_rsa", "private\r\ntext\r"),
        ("id_rsa.pub", "public\r\n"),
        ("id_ecdsa", "ec"),
        ("id_ecdsa.pub", "ecpublic"),
    ] {
        fs::write(comma.join(name), data).unwrap();
    }
    let key = get_key_pair(dir.path()).unwrap().unwrap();
    assert_eq!(key.algorithm, jsonwebtoken::Algorithm::RS256);
    assert_eq!(key.private, "private\ntext\n");
    assert_eq!(key.public, "public\n");
    fs::remove_file(comma.join("id_rsa.pub")).unwrap();
    assert_eq!(
        get_key_pair(dir.path()).unwrap().unwrap().algorithm,
        jsonwebtoken::Algorithm::ES256
    );
}
#[test]
fn empty_public_is_retained_and_invalid_private_fails_during_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("comma")).unwrap();
    fs::write(dir.path().join("comma/id_rsa"), "unused").unwrap();
    fs::write(dir.path().join("comma/id_rsa.pub"), "").unwrap();
    assert!(get_key_pair(dir.path()).unwrap().unwrap().public.is_empty());
    fs::write(dir.path().join("comma/id_rsa"), [255]).unwrap();
    assert!(get_key_pair(dir.path()).is_err());
}
