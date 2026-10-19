use openpilot_jetlink::{owner::GadgetOwner, Error};
#[test]
fn onroad_owner_can_release_and_reconnect_but_cannot_steal_egpu() {
    let mut owner = GadgetOwner::default();
    let mut setups = 0;
    owner
        .enable(false, || {
            setups += 1;
            Ok(())
        })
        .unwrap();
    owner
        .enable(false, || {
            setups += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(setups, 1);
    assert!(owner.disable(|| Err(Error::Closed)).is_err());
    assert!(owner.enabled());
    owner.disable(|| Ok(())).unwrap();
    assert!(!owner.enabled());
    assert!(owner
        .enable(true, || panic!("eGPU ownership was bypassed"))
        .is_err());
    owner
        .enable(false, || {
            setups += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(setups, 2);
}
