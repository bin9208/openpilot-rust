use super::{GuiConfig, Result};
use openpilot_sdl2::gui::Tk;
use std::path::Path;
/// # Errors
/// Returns the external dialog initialization or interpreter error.
pub fn choose_log_file(config: &GuiConfig, initial_folder: &str) -> Result<Option<String>> {
    let mut tk = Tk::load(&config.tcl_library, &config.tk_library)?;
    for args in [
        vec!["wm", "withdraw", "."],
        vec!["wm", "attributes", ".", "-topmost", "1"],
    ] {
        tk.eval(&args.into_iter().map(str::to_owned).collect::<Vec<_>>())?;
    }
    let mut args = vec![
        "tk_getOpenFile".into(),
        "-title".into(),
        "Open cluster replay log".into(),
        "-filetypes".into(),
        "{{Openpilot logs} {rlog.zst qlog.zst rlog.bz2 qlog.bz2}} {{All files} {*.*}}".into(),
    ];
    if Path::new(initial_folder).is_dir() {
        args.extend(["-initialdir".into(), initial_folder.into()]);
    }
    let selected = tk.eval(&args);
    let destroyed = tk.eval(&["destroy".into(), ".".into()]);
    match (selected, destroyed) {
        (Ok(value), Ok(_)) => Ok((!value.is_empty()).then_some(value)),
        (Err(error), _) | (_, Err(error)) => Err(error.into()),
    }
}
