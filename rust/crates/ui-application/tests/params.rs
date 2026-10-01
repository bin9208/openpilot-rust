use openpilot_ui_application::params::{
    store::{Mutation, Store},
    Read,
};
#[test]
fn private_params_fifo_flush_preserves_last_write_and_can_restart_worker(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = Store::new(openpilot_params::Params::open(root.path(), "ui-test")?);
    for i in 0..100 {
        store.put_nonblocking(Mutation {
            key: "ShowModelView".into(),
            value: i.to_string().into_bytes(),
        })?;
    }
    store.flush()?;
    assert_eq!(store.integer("ShowModelView")?, 99);
    store.put_bool_nonblocking("ScreenRecord", true)?;
    store.flush()?;
    assert!(store.boolean("ScreenRecord")?);
    store.remove("GithubSshKeys")?;
    Ok(())
}
