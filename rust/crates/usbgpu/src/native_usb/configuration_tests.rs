use super::{copy_configuration, RawConfiguration};
use crate::native_usb::metadata_abi::{RawEndpoint, RawInterface, RawInterfaceGroup};
use std::ptr;

#[test]
fn copied_tree_keeps_descriptor_order_and_owns_bytes() {
    let copied = {
        let endpoints = [
            RawEndpoint {
                address: 0x84,
                attributes: 3,
                packet_size: 64,
                ..Default::default()
            },
            RawEndpoint {
                address: 7,
                attributes: 1,
                packet_size: 512,
                ..Default::default()
            },
            RawEndpoint {
                address: 0x81,
                attributes: 2,
                packet_size: 1024,
                ..Default::default()
            },
        ];
        let alternates = [
            RawInterface {
                number: 0,
                alternate: 2,
                endpoints_count: 3,
                endpoints: endpoints.as_ptr(),
                ..Default::default()
            },
            RawInterface {
                number: 0,
                alternate: 0,
                ..Default::default()
            },
        ];
        let groups = [RawInterfaceGroup {
            alternates: alternates.as_ptr(),
            count: 2,
        }];
        let raw = RawConfiguration {
            value: 7,
            interfaces_count: 1,
            interfaces: groups.as_ptr(),
            ..Default::default()
        };
        // SAFETY: categories3/6/10; all declared arrays are aligned initialized stack values live through the copy.
        unsafe { copy_configuration(&raw) }.unwrap()
    };
    assert_eq!(copied.value, 7);
    assert_eq!(
        copied
            .interfaces
            .iter()
            .map(|i| (i.number, i.alternate))
            .collect::<Vec<_>>(),
        [(0, 2), (0, 0)]
    );
    assert_eq!(
        copied.interfaces[0]
            .endpoints
            .iter()
            .map(|e| (e.address, e.attributes, e.packet_size))
            .collect::<Vec<_>>(),
        [(0x84, 3, 64), (7, 1, 512), (0x81, 2, 1024)]
    );
}

#[test]
fn rejects_null_nonempty_and_negative_nested_counts() {
    let raw = RawConfiguration {
        interfaces_count: 1,
        ..Default::default()
    };
    // SAFETY: category10; deliberately null nonempty array is rejected before dereference.
    assert!(unsafe { copy_configuration(&raw) }.is_err());
    let alternate = RawInterface {
        endpoints_count: 1,
        ..Default::default()
    };
    for group in [
        RawInterfaceGroup {
            count: -1,
            ..Default::default()
        },
        RawInterfaceGroup {
            count: 1,
            ..Default::default()
        },
        RawInterfaceGroup {
            count: 1,
            alternates: &alternate,
        },
    ] {
        let raw = RawConfiguration {
            interfaces_count: 1,
            interfaces: &group,
            ..Default::default()
        };
        // SAFETY: category10; each tree is freshly borrowed and retained; negative/null counts reject before reads.
        assert!(unsafe { copy_configuration(&raw) }.is_err());
    }
}

#[test]
fn empty_tree_never_dereferences_null_arrays() {
    let raw = RawConfiguration {
        value: 3,
        ..Default::default()
    };
    // SAFETY: category10; zero counts allow null arrays and no memory is read through them.
    let copied = unsafe { copy_configuration(&raw) }.unwrap();
    assert_eq!(copied.value, 3);
    assert!(copied.interfaces.is_empty());
    // SAFETY: category10; empty slices don't dereference the supplied null pointer.
    assert!(unsafe { super::checked_slice::<u8>(ptr::null(), 0) }
        .unwrap()
        .is_empty());
}
