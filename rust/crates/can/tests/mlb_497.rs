use openpilot_can::{checksum::Kind, signal::Signal};
use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    start_bit: usize,
    data: Vec<u8>,
}
#[derive(Deserialize)]
struct Vector {
    address: u32,
    expected: Vec<u16>,
}
#[derive(Deserialize)]
struct Corpus {
    payloads: Vec<Payload>,
    vectors: Vec<Vector>,
}

#[test]
fn all_ten_mlb_seeds_match_independent_literal_synthetic_vectors() {
    let corpus: Corpus = serde_json::from_str(include_str!("data/mlb_497.json")).unwrap();
    assert_eq!(corpus.vectors.len(), 10);
    for vector in corpus.vectors {
        assert_eq!(vector.expected.len(), corpus.payloads.len());
        for (payload, expected) in corpus.payloads.iter().zip(vector.expected) {
            let signal = Signal {
                name: "CHECKSUM".into(),
                start_bit: payload.start_bit,
                msb: payload.start_bit + 7,
                lsb: payload.start_bit,
                size: 8,
                signed: false,
                factor: 1.,
                offset: 0.,
                little_endian: true,
                kind: Kind::VolkswagenMlb,
            };
            let mut bytes = payload.data.clone();
            assert_eq!(
                signal
                    .kind
                    .compute(vector.address, &signal, &mut bytes)
                    .unwrap(),
                expected
            );
            assert_eq!(bytes, payload.data);
            // The checksum byte is excluded even when already nonzero.
            bytes[payload.start_bit / 8] ^= 0xff;
            assert_eq!(
                signal
                    .kind
                    .compute(vector.address, &signal, &mut bytes)
                    .unwrap(),
                expected
            );
        }
    }
}
