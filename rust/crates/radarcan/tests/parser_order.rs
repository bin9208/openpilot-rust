use openpilot_can::{dbc::Dbc, parser::Parser, Frame, Packet};
use std::sync::Arc;

#[test]
fn successful_arrivals_are_unique_per_update_and_exclude_rejected_frames() {
    let dbc = Arc::new(Dbc::parse("order", "BO_ 706 FIRST: 1 XXX\n SG_ VALUE : 0|8@1+ (1,0) [0|255] \"\" XXX\nBO_ 714 SECOND: 1 XXX\n SG_ VALUE : 0|8@1+ (1,0) [0|255] \"\" XXX\n").unwrap());
    let mut parser = Parser::new(dbc, 1, 1);
    parser.add_address(706, Some(100.), false, 1).unwrap();
    parser.add_address(714, Some(100.), false, 1).unwrap();
    let frame = |address, data, bus| Frame { address, data, bus };
    parser
        .update(&[Packet {
            mono_time: 10,
            frames: vec![
                frame(706, vec![0; 65], 1),
                frame(714, vec![2], 1),
                frame(706, vec![1], 0),
                frame(706, vec![3], 1),
                frame(714, vec![4], 1),
            ],
        }])
        .unwrap();
    assert_eq!(parser.successful_addresses(), [714, 706]);
    assert_eq!(parser.signal("SECOND", "VALUE").unwrap(), 4.);
    parser
        .update(&[Packet {
            mono_time: 20,
            frames: vec![frame(706, vec![5], 1)],
        }])
        .unwrap();
    assert_eq!(parser.successful_addresses(), [706]);
    parser.update(&[]).unwrap();
    assert!(parser.successful_addresses().is_empty());
}
