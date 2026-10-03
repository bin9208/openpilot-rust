use libloading::Library;
use openpilot_panda_usb::{raw::Context, Api};
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    io::{self, BufRead},
    path::Path,
};

fn run(input: &Value, api: std::sync::Arc<Api>) -> Result<Value, Box<dyn std::error::Error>> {
    let context = Context::new(api)?;
    let devices = context.devices()?;
    let mut output = Vec::new();
    for index in 0..devices.len() {
        let device = devices.get(index).ok_or("missing fixture device")?;
        let descriptor = device.descriptor()?;
        let mut handle = device.open()?;
        let serial = handle.ascii_string(descriptor.serial_index)?;
        if input["claim"].as_bool() == Some(true) {
            handle.auto_detach(true)?;
            handle.claim(0)?;
        }
        let mut values = Vec::new();
        for op in input["operations"].as_array().into_iter().flatten() {
            let request = op["request"].as_u64().unwrap_or(0) as u8;
            let kind = op["kind"].as_u64().unwrap_or(0) as u8;
            let value = op["value"].as_u64().unwrap_or(0) as u16;
            let index = op["index"].as_u64().unwrap_or(0) as u16;
            let timeout = op["timeout"].as_u64().unwrap_or(0) as u32;
            let data: Vec<u8> =
                serde_json::from_value(op.get("data").cloned().unwrap_or(json!([])))?;
            let result = match op["op"].as_str().ok_or("missing operation")? {
                "read" => json!(handle.control_read(
                    kind,
                    request,
                    value,
                    index,
                    op["length"].as_u64().unwrap_or(0) as usize,
                    timeout
                )?),
                "write" => {
                    json!(handle.control_write(kind, request, value, index, &data, timeout)?)
                }
                "bulk" => json!(handle.bulk_write(request, &data, timeout)?),
                "string" => json!(handle.string(request, index)?),
                _ => return Err("unknown operation".into()),
            };
            values.push(result);
        }
        output.push(json!({"serial":serial,"vendor":descriptor.vendor,"product":descriptor.product,"bcd":descriptor.bcd,"values":values}));
    }
    Ok(json!(output))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("owned ABI fixture path required")?;
    // SAFETY: this test requires the owned libusb fixture, whose C ABI and lifetime match these declarations.
    let (library, api) = unsafe { (Library::new(&path)?, Api::load(Path::new(&path))?) };
    // SAFETY: the fixture exports synchronous C-string entry points; library owns their lifetime.
    let (begin, finish) = unsafe {
        (
            *library.get::<unsafe extern "C" fn(*const c_char)>(b"raw_fixture_begin\0")?,
            *library.get::<unsafe extern "C" fn() -> *const c_char>(b"fixture_finish\0")?,
        )
    };
    for line in io::stdin().lock().lines() {
        let input = line?;
        let raw = CString::new(input.as_bytes())?;
        // SAFETY: begin copies this live input and finish returns a fixture-owned string copied before the next call.
        let output = unsafe {
            begin(raw.as_ptr());
            let result = run(&serde_json::from_str(&input)?, api.clone());
            let calls: Value = serde_json::from_slice(CStr::from_ptr(finish()).to_bytes())?;
            match result {
                Ok(values) => json!({"ok":true,"values":values,"trace":calls}),
                Err(error) => json!({"ok":false,"error":error.to_string(),"trace":calls}),
            }
        };
        println!("{output}");
    }
    Ok(())
}
