use openpilot_carrot_server::screenrecord::info;
#[test]
fn bounded_iso_boxes_parse_video_and_tolerate_truncation() {
    fn atom(kind: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
        let mut v = u32::try_from(body.len() + 8)
            .unwrap()
            .to_be_bytes()
            .to_vec();
        v.extend(kind);
        v.extend(body);
        v
    }
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("20261015-010203.mp4");
    let mut mdhd = vec![0; 12];
    mdhd.extend(1000u32.to_be_bytes());
    mdhd.extend(10000u32.to_be_bytes());
    let mut hdlr = vec![0; 8];
    hdlr.extend(b"vide");
    let mut stsz = vec![0; 8];
    stsz.extend(300u32.to_be_bytes());
    let mut stsd = vec![0; 40];
    stsd.extend(1920u16.to_be_bytes());
    stsd.extend(1080u16.to_be_bytes());
    let stbl = atom(b"stbl", [atom(b"stsz", stsz), atom(b"stsd", stsd)].concat());
    let mdia = atom(
        b"mdia",
        [
            atom(b"hdlr", hdlr),
            atom(b"mdhd", mdhd),
            atom(b"minf", stbl),
        ]
        .concat(),
    );
    std::fs::write(&file, atom(b"moov", atom(b"trak", mdia))).unwrap();
    let value = info::probe(&file).unwrap();
    assert_eq!(value.get("width").int().unwrap(), 1920.into());
    assert_eq!(value.get("height").int().unwrap(), 1080.into());
    assert!(value.get("codec").text_eq("H.264"));
    assert_eq!(
        value.get("durationSeconds"),
        &openpilot_carrot_server::Value::Float(10.0)
    );
    assert_eq!(
        value.get("fps"),
        &openpilot_carrot_server::Value::Float(30.0)
    );
    assert!(value.has("startEpoch"));
    std::fs::write(&file, b"short").unwrap();
    assert!(!info::probe(&file).unwrap().has("width"));
}
