use openpilot_usbgpu::{
    native_usb::Usb,
    transport::{Control, Setup, Transfer, Transport},
};
fn main() {
    let mode = std::env::var("USB_FIXTURE_MODE").unwrap();
    if mode == "open_error" {
        assert!(Usb::open(0xadd1, 1, 0).is_err());
        println!("PASS open_error");
        return;
    }
    if mode == "missing" {
        assert!(Usb::open(0xadd1, 2, 0).unwrap().is_none());
        println!("PASS missing");
        return;
    }
    let mut usb = Usb::open(0xadd1, 1, 0).unwrap().unwrap();
    let description = usb.describe().unwrap();
    assert_eq!((description.bus, description.address), (7, 9));
    assert_eq!(description.product, b"test");
    assert_eq!(usb.setup(Setup::Claim, 0, 0).unwrap(), 0);
    assert_eq!(usb.streams(&[0x81, 0x02], 31).unwrap(), 31);
    let mut data = [0; 16];
    let code = usb
        .control(
            Control {
                kind: 0xc0,
                request: 0xe4,
                value: 0,
                index: 0,
                timeout_ms: 2000,
            },
            &mut data,
        )
        .unwrap();
    if mode == "control_error" {
        assert_eq!(code, -1);
    } else {
        assert_eq!(code, 16);
        assert_eq!(data, [0xa5; 16]);
    }
    let result = usb.bulk(0x82, &mut data, 1000).unwrap();
    assert_eq!(
        (result.code, result.actual),
        if mode == "partial" { (-1, 8) } else { (0, 16) }
    );
    let mut transfers = (0..3)
        .map(|_| Transfer::new(0x82, Some(1), vec![0; 32]))
        .collect::<Vec<_>>();
    let result = usb.batch(&mut transfers);
    if mode == "submit_error" || mode == "event_error" {
        assert!(result.is_err());
        assert!(transfers.iter().all(|t| t.data.len() == 32));
    } else {
        result.unwrap();
        assert!(transfers
            .iter()
            .all(|t| t.actual == 32 && t.status == 0 && t.data == vec![0x3c; 32]));
    }
    drop(usb);
    println!("PASS {mode}");
}
