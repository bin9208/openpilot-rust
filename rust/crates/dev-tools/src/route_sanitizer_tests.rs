//! Authored synthetic fixtures only. These tests have NOT been executed.
use super::*;
use capnp::introspect::{Introspect, TypeVariant};
use capnp::message::ReaderOptions;
use capnp::serialize::{read_message, write_message_to_words};
use capnp::traits::HasStructSize;
use openpilot_cereal::car_capnp::car_params;
use std::cell::Cell;

fn fixture(service: &str, vin: &str) -> Builder<HeapAllocator> {
    let mut message = Builder::new_default();
    let mut root = message.init_root::<event::Builder<'_>>();
    root.set_log_mono_time(0x1234_5678_9abc_def0);
    root.set_valid(false);
    match service {
        "carParams" => {
            let mut params = root.init_car_params();
            params.set_car_vin(vin);
            params.set_brand("synthetic-brand");
            params.set_car_fingerprint("SYNTHETIC_FIXTURE_ONLY");
            params.set_mass(1234.5);
            params.set_flags(0x5a5a_0101);
            let mut points = params.init_longitudinal_tuning().init_kp_b_p(2);
            points.set(0, 1.25);
            points.set(1, 9.5);
        }
        "can" => {
            let mut data = root.init_can(1).get(0);
            data.set_address(0x123);
            data.set_dat(&[1, 2, 3, 4]);
        }
        "pandaStates" => {
            root.init_panda_states(0);
        }
        "pandaStateDEPRECATED" => {
            root.init_panda_state_d_e_p_r_e_c_a_t_e_d();
        }
        "logMessage" => root.set_log_message("synthetic-filtered"),
        _ => panic!("synthetic service not authored"),
    }
    message
}

fn owned(service: &str) -> LogMessage<'static> {
    LogMessage::Owned(OwnedEvent::new(fixture(service, "prefixABCDEF")))
}

fn vin(message: event::Reader<'_>) -> String {
    let event::CarParams(params) = message.which().unwrap() else {
        panic!("not synthetic carParams")
    };
    params
        .unwrap()
        .get_car_vin()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}

fn canonical(reader: event::Reader<'_>) -> Vec<u8> {
    let mut message = Builder::new_default();
    message.set_root_canonical(reader).unwrap();
    write_message_to_words(&message)
}

#[test]
fn vin_slices_unicode_codepoints_and_always_adds_exactly_six_x() {
    // These literals are source-formula cases, not captured Python observations.
    for (input, expected) in [
        ("", "XXXXXX"),
        ("a", "XXXXXX"),
        ("abcde", "XXXXXX"),
        ("abcdef", "XXXXXX"),
        ("abcdefg", "aXXXXXX"),
        ("prefixABCDEF", "prefixXXXXXX"),
        ("前缀甲乙丙丁戊己", "前缀XXXXXX"),
        ("é🦀甲乙丙丁戊己", "é🦀XXXXXX"),
        ("A\u{301}BCDEF", "AXXXXXX"),
        ("a\0bcdef", "aXXXXXX"),
    ] {
        assert_eq!(sanitize_vin(input), expected);
    }
}

#[test]
fn service_roster_and_order_are_exact() {
    assert_eq!(
        PRESERVE_SERVICES,
        ["can", "carParams", "pandaStates", "pandaStateDEPRECATED"]
    );
    let input = [
        "logMessage",
        "pandaStates",
        "can",
        "carParams",
        "pandaStateDEPRECATED",
    ];
    let output = sanitize(
        input
            .into_iter()
            .map(|name| Ok::<_, capnp::Error>(owned(name))),
    )
    .map(|message| {
        let message = message.unwrap();
        let service = match message.as_reader().unwrap().which().unwrap() {
            event::Can(_) => "can",
            event::CarParams(_) => "carParams",
            event::PandaStates(_) => "pandaStates",
            event::PandaStateDEPRECATED(_) => "pandaStateDEPRECATED",
            _ => panic!("unpreserved service escaped"),
        };
        service
    })
    .collect::<Vec<_>>();
    assert_eq!(output, input[1..]);
}

#[test]
fn car_params_copy_preserves_metadata_fields_and_original() {
    let original = fixture("carParams", "prefixABCDEF");
    let before = write_message_to_words(&original);
    let output =
        sanitize_msg(LogMessage::Borrowed(original.get_root_as_reader().unwrap())).unwrap();
    let reader = output.as_reader().unwrap();
    assert_eq!(reader.get_log_mono_time(), 0x1234_5678_9abc_def0);
    assert!(!reader.get_valid());
    let event::CarParams(params) = reader.which().unwrap() else {
        panic!("changed union")
    };
    let params = params.unwrap();
    assert_eq!(
        params.get_car_vin().unwrap().to_str().unwrap(),
        "prefixXXXXXX"
    );
    assert_eq!(
        params.get_brand().unwrap().to_str().unwrap(),
        "synthetic-brand"
    );
    assert_eq!(
        params.get_car_fingerprint().unwrap().to_str().unwrap(),
        "SYNTHETIC_FIXTURE_ONLY"
    );
    assert_eq!(params.get_mass(), 1234.5);
    assert_eq!(params.get_flags(), 0x5a5a_0101);
    let points = params
        .get_longitudinal_tuning()
        .unwrap()
        .get_kp_b_p()
        .unwrap();
    assert_eq!([points.get(0), points.get(1)], [1.25, 9.5]);
    assert_eq!(write_message_to_words(&original), before);
    assert_eq!(vin(original.get_root_as_reader().unwrap()), "prefixABCDEF");
}

#[test]
fn owned_sanitized_arena_outlives_the_original() {
    let copy = {
        let original = fixture("carParams", "prefixABCDEF");
        let result =
            sanitize_msg(LogMessage::Borrowed(original.get_root_as_reader().unwrap())).unwrap();
        let LogMessage::Owned(copy) = result else {
            panic!("missing owned copy")
        };
        copy
    };
    assert_eq!(vin(copy.as_reader().unwrap()), "prefixXXXXXX");
}

#[test]
fn non_car_params_borrowed_and_owned_arenas_pass_through_unchanged() {
    for service in ["can", "pandaStates", "pandaStateDEPRECATED", "logMessage"] {
        let original = fixture(service, "");
        let before = write_message_to_words(&original);
        let output =
            sanitize_msg(LogMessage::Borrowed(original.get_root_as_reader().unwrap())).unwrap();
        let LogMessage::Borrowed(reader) = output else {
            panic!("borrowed message copied")
        };
        // The reader still points into the original arena, not a rebuilt Event.
        assert_eq!(
            canonical(reader),
            canonical(original.get_root_as_reader().unwrap())
        );
        assert_eq!(write_message_to_words(&original), before);
        let original = OwnedEvent::new(fixture(service, ""));
        let address = (&*original.message) as *const Builder<HeapAllocator>;
        let output = sanitize_msg(LogMessage::Owned(original)).unwrap();
        let LogMessage::Owned(output) = output else {
            panic!("owned message lost")
        };
        assert_eq!((&*output.message) as *const Builder<HeapAllocator>, address);
        assert_eq!(write_message_to_words(&output.message), before);
    }
}

#[test]
fn filter_then_map_is_lazy_and_never_prefetches() {
    let pulls = Cell::new(0);
    let mut input = [owned("logMessage"), owned("can"), owned("carParams")].into_iter();
    let source = std::iter::from_fn(|| {
        pulls.set(pulls.get() + 1);
        input.next().map(Ok::<_, capnp::Error>)
    });
    let mut output = sanitize(source);
    assert_eq!(pulls.get(), 0);
    assert!(matches!(
        output
            .next()
            .unwrap()
            .unwrap()
            .as_reader()
            .unwrap()
            .which()
            .unwrap(),
        event::Can(_)
    ));
    assert_eq!(pulls.get(), 2);
    assert_eq!(
        vin(output.next().unwrap().unwrap().as_reader().unwrap()),
        "prefixXXXXXX"
    );
    assert_eq!(pulls.get(), 3);
    assert!(output.next().is_none());
    assert_eq!(pulls.get(), 4);
}

#[derive(Debug)]
enum InputError {
    Upstream(&'static str),
    Cereal(capnp::Error),
}
impl From<capnp::Error> for InputError {
    fn from(error: capnp::Error) -> Self {
        Self::Cereal(error)
    }
}

#[test]
fn upstream_error_is_observed_at_its_next_and_does_not_fuse() {
    let input = [
        Ok(owned("can")),
        Err(InputError::Upstream("synthetic upstream")),
        Ok(owned("carParams")),
    ];
    let mut output = sanitize(input);
    assert!(output.next().unwrap().is_ok());
    assert!(matches!(
        output.next().unwrap(),
        Err(InputError::Upstream("synthetic upstream"))
    ));
    assert_eq!(
        vin(output.next().unwrap().unwrap().as_reader().unwrap()),
        "prefixXXXXXX"
    );
}

// Test-only near-pointer construction. Never consumes a file, route or VIN.
// Every offset/dimension is taken from the synthetic arena/generated schema.
fn words(message: &Builder<HeapAllocator>) -> Vec<u64> {
    let segments = message.get_segments_for_output();
    assert_eq!(segments.len(), 1, "test fixture must use one segment");
    segments[0]
        .chunks_exact(8)
        .map(|w| u64::from_le_bytes(w.try_into().unwrap()))
        .collect()
}
fn target(data: &[u64], pointer: usize) -> usize {
    let offset = (data[pointer] as u32 as i32) >> 2;
    usize::try_from(pointer as i64 + 1 + i64::from(offset)).unwrap()
}
fn near_pointer(pointer: usize, destination: usize, high: u64, kind: u64) -> u64 {
    let offset = i32::try_from(destination as i64 - pointer as i64 - 1).unwrap();
    assert!((-(1 << 29)..(1 << 29)).contains(&offset));
    high | u64::from(((offset as u32) << 2) | kind as u32)
}
fn struct_parts(data: &[u64], pointer: usize) -> (usize, usize, usize) {
    assert_eq!(data[pointer] & 3, 0);
    (
        target(data, pointer),
        ((data[pointer] >> 32) & 0xffff) as usize,
        (data[pointer] >> 48) as usize,
    )
}
fn extended_struct(data: &mut Vec<u64>, pointer: usize, marker: u64, text: &str) {
    let (old, d, p) = struct_parts(data, pointer);
    let original = data[old..old + d + p].to_vec();
    let start = data.len();
    data.extend_from_slice(&original[..d]);
    data.push(marker);
    for i in 0..p {
        let word = original[d + i];
        let destination = start + d + 1 + i;
        data.push(if word == 0 {
            0
        } else {
            assert!(
                matches!(word & 3, 0 | 1),
                "no far/capability pointer in synthetic fixture"
            );
            near_pointer(
                destination,
                target(data, old + d + i),
                word & !0xffff_ffff,
                word & 3,
            )
        });
    }
    let extra = data.len();
    data.push(0);
    let text_start = data.len();
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let len = bytes.len();
    bytes.resize(len.div_ceil(8) * 8, 0);
    data.extend(
        bytes
            .chunks_exact(8)
            .map(|w| u64::from_le_bytes(w.try_into().unwrap())),
    );
    data[extra] = near_pointer(extra, text_start, ((len as u64) << 35) | (2 << 32), 1);
    data[pointer] = near_pointer(
        pointer,
        start,
        (((d + 1) as u64) << 32) | (((p + 1) as u64) << 48),
        0,
    );
}
fn framed(data: &[u64]) -> Vec<u8> {
    let mut bytes = vec![0; 4]; // one segment: segment count minus one
    bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    for word in data {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}
fn extension(data: &[u64], pointer: usize, known_d: usize, known_p: usize) -> (u64, Vec<u8>) {
    let (start, d, p) = struct_parts(data, pointer);
    assert_eq!((d, p), (known_d + 1, known_p + 1));
    let extra = start + d + known_p;
    assert_eq!(data[extra] & 3, 1);
    assert_eq!((data[extra] >> 32) & 7, 2);
    let len = (data[extra] >> 35) as usize;
    let text_start = target(data, extra);
    let all = data[text_start..]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect::<Vec<_>>();
    (data[start + known_d], all[..len].to_vec())
}

#[test]
fn unknown_event_and_car_params_data_and_pointer_fields_survive() {
    let original = fixture("carParams", "prefixABCDEF");
    let mut data = words(&original);
    let event_size = <event::Builder<'_> as HasStructSize>::STRUCT_SIZE;
    let car_size = <car_params::Builder<'_> as HasStructSize>::STRUCT_SIZE;
    let (root, d, p) = struct_parts(&data, 0);
    assert_eq!(
        (d, p),
        (
            usize::from(event_size.data),
            usize::from(event_size.pointers)
        )
    );
    assert_eq!(p, 1, "the Event union shares its one pointer slot");
    let car_pointer = root + d;
    extended_struct(
        &mut data,
        car_pointer,
        0x1122_3344_5566_7788,
        "synthetic-car-extension",
    );
    extended_struct(
        &mut data,
        0,
        0x8877_6655_4433_2211,
        "synthetic-event-extension",
    );
    let raw = framed(&data);
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let expected = canonical(original.get_root().unwrap());
    let output = sanitize_msg(LogMessage::Borrowed(original.get_root().unwrap())).unwrap();
    assert_eq!(vin(output.as_reader().unwrap()), "prefixXXXXXX");
    let LogMessage::Owned(mut output) = output else {
        panic!("copy absent")
    };
    let data = words(&output.message);
    assert_eq!(
        extension(&data, 0, d, p),
        (
            0x8877_6655_4433_2211,
            b"synthetic-event-extension\0".to_vec()
        )
    );
    let (start, size, _) = struct_parts(&data, 0);
    assert_eq!(
        extension(
            &data,
            start + size,
            usize::from(car_size.data),
            usize::from(car_size.pointers)
        ),
        (0x1122_3344_5566_7788, b"synthetic-car-extension\0".to_vec())
    );
    // Restore just VIN and compare the entire canonical wire graph, not only
    // known fields. Canonicalization is test-only, never in sanitize_msg.
    let event::CarParams(params) = output
        .message
        .get_root::<event::Builder<'_>>()
        .unwrap()
        .which()
        .unwrap()
    else {
        panic!("changed union")
    };
    params.unwrap().set_car_vin("prefixABCDEF");
    assert_eq!(canonical(output.as_reader().unwrap()), expected);
    assert_eq!(vin(original.get_root().unwrap()), "prefixABCDEF");
}

#[test]
fn invalid_vin_utf8_is_lazy_error_and_leaves_original_unmodified() {
    let mut raw = write_message_to_words(&fixture("carParams", "synthetic-utf8-marker"));
    let offset = raw
        .windows(b"synthetic-utf8-marker".len())
        .position(|v| v == b"synthetic-utf8-marker")
        .unwrap();
    raw[offset] = 0xff;
    let before = raw.clone();
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let mut output = sanitize([
        Ok::<_, InputError>(LogMessage::Borrowed(original.get_root().unwrap())),
        Ok(owned("can")),
    ]);
    let error = output.next().unwrap().err().unwrap();
    let InputError::Cereal(error) = error else {
        panic!("wrong error boundary")
    };
    assert!(!error.to_string().is_empty());
    assert!(output.next().unwrap().is_ok());
    assert_eq!(raw, before);
}

#[test]
fn unknown_union_tag_errors_in_filter_and_can_resume_on_next_input() {
    let mut data = words(&fixture("can", ""));
    let TypeVariant::Struct(schema) = event::Owned::introspect().which() else {
        panic!("not struct")
    };
    let capnp::schema_capnp::node::Struct(proto) = capnp::schema::StructSchema::new(schema)
        .get_proto()
        .which()
        .unwrap()
    else {
        panic!("not struct node")
    };
    let discriminator = proto.get_discriminant_offset() as usize;
    let (root, d, _) = struct_parts(&data, 0);
    assert!(discriminator / 4 < d);
    let shift = (discriminator % 4) * 16;
    data[root + discriminator / 4] =
        (data[root + discriminator / 4] & !(0xffff << shift)) | (0xffff << shift);
    let raw = framed(&data);
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let mut output = sanitize([
        Ok::<_, capnp::Error>(LogMessage::Borrowed(original.get_root().unwrap())),
        Ok(owned("can")),
    ]);
    assert!(output.next().unwrap().is_err());
    assert!(output.next().unwrap().is_ok());
}

#[test]
fn malformed_non_car_payload_passes_through_without_deep_copy_or_payload_error() {
    // A tag-only filter must not debit CAN's child traversal budget. Allocate
    // enough for each actual synthetic wire word once, then read the payload
    // only after sanitize returns. Generated which() in filter/map would read
    // the child repeatedly and consume that same reader's limit prematurely.
    let normal = words(&fixture("can", ""));
    let raw = framed(&normal);
    let options = ReaderOptions {
        traversal_limit_in_words: Some(normal.len()),
        ..ReaderOptions::new()
    };
    let original = read_message(raw.as_slice(), options).unwrap();
    let mut output = sanitize([Ok::<_, capnp::Error>(LogMessage::Borrowed(
        original.get_root().unwrap(),
    ))]);
    let message = output.next().unwrap().unwrap();
    let event::Can(payload) = message.as_reader().unwrap().which().unwrap() else {
        panic!("union changed")
    };
    assert_eq!(payload.unwrap().get(0).get_dat().unwrap(), &[1, 2, 3, 4]);

    let mut data = words(&fixture("can", ""));
    let (root, d, p) = struct_parts(&data, 0);
    assert_eq!(p, 1);
    // Use a struct pointer where the source service expects a list.
    data[root + d] = near_pointer(root + d, root, 1 << 32, 0);
    let raw = framed(&data);
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let mut output = sanitize([Ok::<_, capnp::Error>(LogMessage::Borrowed(
        original.get_root().unwrap(),
    ))]);
    let output = output.next().unwrap().unwrap();
    let LogMessage::Borrowed(reader) = output else {
        panic!("payload was copied")
    };
    let event::Can(payload) = reader.which().unwrap() else {
        panic!("union changed")
    };
    assert!(payload.is_err());
}

#[test]
fn cereal_empty_short_and_multibyte_vins_use_the_same_source_slice() {
    for (input, expected) in [
        ("", "XXXXXX"),
        ("abc", "XXXXXX"),
        ("前缀甲乙丙丁戊己", "前缀XXXXXX"),
    ] {
        let original = fixture("carParams", input);
        let output =
            sanitize_msg(LogMessage::Borrowed(original.get_root_as_reader().unwrap())).unwrap();
        assert_eq!(vin(output.as_reader().unwrap()), expected);
        assert_eq!(vin(original.get_root_as_reader().unwrap()), input);
    }
}

#[test]
fn constructor_calls_into_iter_immediately_without_pulling_a_message() {
    struct Source<'a>(&'a Cell<usize>);
    impl IntoIterator for Source<'_> {
        type Item = capnp::Result<LogMessage<'static>>;
        type IntoIter = std::iter::Empty<Self::Item>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.set(self.0.get() + 1);
            std::iter::empty()
        }
    }
    let constructed = Cell::new(0);
    let output = sanitize(Source(&constructed));
    assert_eq!(constructed.get(), 1);
    drop(output);
    assert_eq!(constructed.get(), 1);
}

#[test]
fn deep_copy_error_precedes_reading_invalid_vin_text() {
    let mut data = words(&fixture("carParams", "synthetic-invalid-utf8"));
    let (root, d, _) = struct_parts(&data, 0);
    extended_struct(&mut data, root + d, 7, "synthetic-extra");
    let (params, car_d, car_p) = struct_parts(&data, root + d);
    let unknown = params + car_d + car_p - 1;
    data[unknown] = near_pointer(unknown, data.len() + 100, 1 << 32, 0);
    let mut raw = framed(&data);
    let offset = raw
        .windows(b"synthetic-invalid-utf8".len())
        .position(|v| v == b"synthetic-invalid-utf8")
        .unwrap();
    raw[offset] = 0xff;
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let error = sanitize_msg(LogMessage::Borrowed(original.get_root().unwrap()))
        .err()
        .unwrap();
    assert!(matches!(
        error.kind,
        capnp::ErrorKind::MessageContainsOutOfBoundsPointer
    ));
}

#[test]
fn filtered_service_does_not_materialize_its_invalid_text_payload() {
    let mut data = words(&fixture("logMessage", ""));
    let (root, d, p) = struct_parts(&data, 0);
    assert_eq!(p, 1);
    data[root + d] = near_pointer(root + d, root, 1 << 32, 0);
    let raw = framed(&data);
    let original = read_message(raw.as_slice(), ReaderOptions::new()).unwrap();
    let mut output = sanitize([
        Ok::<_, capnp::Error>(LogMessage::Borrowed(original.get_root().unwrap())),
        Ok(owned("can")),
    ]);
    let message = output.next().unwrap().unwrap();
    assert!(matches!(
        message.as_reader().unwrap().which().unwrap(),
        event::Can(_)
    ));
    assert!(output.next().is_none());
}
