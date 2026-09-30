use openpilot_logging::{context::GlobalContext, Fields, Value};
fn fields(values: &[(&str, &str)]) -> Fields {
    values
        .iter()
        .map(|(k, v)| (String::from(*k), Value::Text(String::from(*v))))
        .collect()
}
#[test]
fn global_wins_and_scope_restores_when_unwinding() {
    // Given source local/global overlapping keys.
    let global = GlobalContext::default();
    global.bind(fields(&[("key", "global")])).unwrap();
    let local = global.local();
    local.bind(fields(&[("key", "local"), ("stable", "before")]));
    // When a nested scoped context panics after binding more fields.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = local.scoped(fields(&[("stable", "scope"), ("inner", "yes")]));
        assert_eq!(
            local.snapshot().unwrap()["key"],
            Value::Text("global".into())
        );
        local.bind(fields(&[("temporary", "yes")]));
        panic!("intentional scope unwind");
    }));
    // Then every scoped/bound addition is undone and the old local context survives.
    assert!(result.is_err());
    let context = local.snapshot().unwrap();
    assert_eq!(context["stable"], Value::Text("before".into()));
    assert!(!context.contains_key("inner") && !context.contains_key("temporary"));
}
#[test]
fn local_context_is_isolated_when_factory_context_is_shared() {
    // Given shared process context and a main-thread local binding.
    let global = GlobalContext::default();
    let main = global.local();
    main.bind(fields(&[("thread", "main")]));
    // When a second thread binds a local context and updates a global field.
    let child = global.clone();
    let snapshot = std::thread::spawn(move || {
        let local = child.local();
        local.bind(fields(&[("thread", "worker")]));
        child.bind(fields(&[("shared", "updated")])).unwrap();
        local.snapshot().unwrap()
    })
    .join()
    .unwrap();
    // Then local fields differ but the global update is shared.
    assert_eq!(snapshot["thread"], Value::Text("worker".into()));
    assert_eq!(
        main.snapshot().unwrap()["thread"],
        Value::Text("main".into())
    );
    assert_eq!(
        main.snapshot().unwrap()["shared"],
        Value::Text("updated".into())
    );
}
