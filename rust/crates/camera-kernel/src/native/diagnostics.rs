use std::sync::OnceLock;

#[derive(Debug)]
pub enum KernelDiagnostic {
    Message {
        debug: bool,
        text: String,
    },
    CameraControl {
        opcode: u32,
        errno: i32,
    },
    SyncControl {
        id: u32,
        errno: i32,
        transport: i32,
        kernel: i32,
    },
}

pub(super) fn message(debug: bool, text: String) {
    report(KernelDiagnostic::Message { debug, text });
}

static HANDLER: OnceLock<fn(KernelDiagnostic)> = OnceLock::new();

pub fn set_diagnostic_handler(handler: fn(KernelDiagnostic)) {
    let _ = HANDLER.set(handler);
}

pub(super) fn report(value: KernelDiagnostic) {
    if let Some(handler) = HANDLER.get() {
        handler(value);
    }
}
