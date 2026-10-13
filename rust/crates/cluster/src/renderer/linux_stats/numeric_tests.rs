use super::*;

#[test]
fn ratios_round_ties_to_even_and_preserve_subnormal_boundaries() {
    let power = BigInt::one() << 1075_usize;
    assert_eq!(ratio(&BigInt::one(), &power).unwrap().to_bits(), 0);
    assert_eq!(ratio(&BigInt::from(3), &power).unwrap().to_bits(), 2);
    let base = BigInt::one() << 53_usize;
    assert_eq!(
        ratio(&(&base + 1), &base).unwrap().to_bits(),
        1.0_f64.to_bits()
    );
    assert_eq!(
        ratio(&(&base + 3), &base).unwrap().to_bits(),
        1.0_f64.to_bits() + 2
    );
    assert_eq!(
        ratio(&BigInt::from(-1), &BigInt::from(4))
            .unwrap()
            .to_bits(),
        (-0.25_f64).to_bits()
    );
    assert!(ratio(&(BigInt::one() << 1024_usize), &BigInt::one()).is_err());
    assert!(ratio(&BigInt::one(), &BigInt::zero()).is_err());
}

#[test]
fn signed_large_memory_numbers_roundtrip_as_json_numbers() {
    let value = MemoryBytes::from_integer(-(BigInt::one() << 500_usize));
    let encoded = serde_json::to_string(&value).unwrap();
    assert_eq!(encoded, value.0.to_string());
    assert_eq!(
        serde_json::from_str::<MemoryBytes>(&encoded).unwrap(),
        value
    );
    assert!(serde_json::from_str::<MemoryBytes>("1.5").is_err());
    assert!(serde_json::from_str::<MemoryBytes>("\"100\"").is_err());
    let small = serde_json::from_value::<MemoryBytes>(serde_json::json!(123)).unwrap();
    assert_eq!(serde_json::to_value(small).unwrap(), serde_json::json!(123));
    assert!(MemoryBytes::from_integer(BigInt::one() << 1024_usize)
        .gibibytes()
        .is_err());
}
