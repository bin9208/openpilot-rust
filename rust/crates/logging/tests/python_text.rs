use openpilot_logging::record::{Level, Record};
use openpilot_logging::{Fields, PythonText, Value};

#[test]
fn python_text_preserves_surrogates_and_console_backslashreplace() {
    let text = PythonText::new(vec![0x61, 0xdcff, 0xd800, 0x1f600, 10, 34]).unwrap();
    assert_eq!(text.console(), "a\\udcff\\ud800😀\n\"");
    let record = Record::python_text(Level::Error, text.clone());
    assert_eq!(record.message, Value::PythonText(text));
    let fields: Fields = [("msg".into(), record.message)].into_iter().collect();
    assert_eq!(
        fields.to_json().unwrap(),
        r#"{"msg": "a\udcff\ud800\ud83d\ude00\n\""}"#
    );
    assert!(PythonText::new(vec![0x110000]).is_err());
}
