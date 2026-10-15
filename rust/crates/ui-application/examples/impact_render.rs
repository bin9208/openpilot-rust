#[allow(dead_code)]
#[path = "support/runtime_context.rs"]
mod fixture;
use openpilot_impact_dashcam::{FEEDBACK_KEY, NOTICE_KEY, REBOOT_KEY};
use openpilot_ui_application::{impact_prompt::Prompt, params::Read};
use openpilot_ui_framework::{
    application::{Application, ApplicationConfig},
    button::Button,
    geometry::{MouseEvent, Point},
    widget::WidgetHandle,
};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(args.get(1).ok_or("root")?);
    let output = Path::new(args.get(2).ok_or("output")?);
    std::fs::create_dir_all(output)?;
    let params = openpilot_params::Params::for_runtime()?;
    for key in openpilot_params::KEYS {
        if let Some(value) = key.default {
            params.put(key.name, value.as_bytes())?;
        }
    }
    let language = args.get(3).map_or("en", String::as_str);
    let mut config = ApplicationConfig::for_runtime(root, "Impact prompt fixture")?;
    config.language = language.into();
    let mut app = Application::new(config)?;
    let (mut context, _) = fixture::context(root, output, &app)?;
    context.translations = openpilot_ui_application::context::Translations::new(
        openpilot_ui_framework::multilang::Multilang::new(
            &root.join("openpilot/selfdrive/ui/translations"),
            Some(language),
        )?,
    );
    let clicks = Rc::new(Cell::new(0));
    let tally = clicks.clone();
    let mut button = Button::new("");
    button.state.click = Some(Box::new(move || tally.set(tally.get() + 1)));
    app.push(WidgetHandle::new(button));
    let prompt = RefCell::new(Prompt::new(context.clone()));
    let mut rows = Vec::new();
    for index in 0..14u32 {
        let now = 1.0 + f64::from(index) * 0.1;
        context.ui.borrow_mut().started = index != 13;
        context.params.put_bool(REBOOT_KEY, index == 12)?;
        if index >= 3 {
            context.memory.put(
                NOTICE_KEY,
                json!({"token":if index<7{"one"}else{"two"},"created":0.0})
                    .to_string()
                    .as_bytes(),
            )?;
        }
        context.memory.remove(FEEDBACK_KEY)?;
        let mut event = capnp::message::Builder::new_default();
        let mut header = event.init_root::<openpilot_cereal::log_capnp::event::Builder>();
        header.set_valid(index != 10);
        let mut ss = header.init_selfdrive_state();
        if index == 7 {
            ss.set_alert_size(openpilot_cereal::log_capnp::selfdrive_state::AlertSize::Full);
        }
        if index == 8 {
            ss.set_alert_status(
                openpilot_cereal::log_capnp::selfdrive_state::AlertStatus::Critical,
            );
        }
        context.messages.borrow_mut().state.update(
            if index == 9 { now - 1.0 } else { now },
            &[capnp::serialize::write_message_to_words(&event)],
        )?;
        let pressed = matches!(index, 1 | 4);
        let down = matches!(index, 1 | 4 | 5);
        let released = matches!(index, 2 | 6);
        app.render_cycle_with_input(
            |events| {
                events.clear();
                if pressed || down || released {
                    events.push(MouseEvent {
                        pos: Point { x: 100.0, y: 100.0 },
                        slot: 0,
                        pressed,
                        down,
                        released,
                        time: now,
                    });
                }
                prompt.borrow_mut().input(events).map_err(Into::into)
            },
            |_, _, _| Ok(()),
            |_, canvas, _, _, rendered| {
                if rendered {
                    let width = canvas.renderer.dimensions().0;
                    prompt.borrow_mut().render(now, width, canvas)?;
                    canvas
                        .renderer
                        .screenshot(&output.join(format!("frame-{index:02}.png")))?;
                }
                Ok(())
            },
        )?;
        let feedback = context
            .memory
            .bytes(FEEDBACK_KEY)?
            .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes))
            .transpose()?;
        if matches!(index, 3 | 11) {
            assert_eq!(
                feedback,
                Some(json!({"token":if index==3{"one"}else{"two"},"visible":now}))
            );
        }
        if (4..=6).contains(&index) {
            assert_eq!(feedback, Some(json!({"token":"one","cancel":true})));
        }
        if matches!(index, 7..=10 | 12 | 13) {
            assert_eq!(feedback, None);
        }
        if index >= 2 {
            assert_eq!(clicks.get(), 1, "cancelled touch leaked at {index}");
        }
        rows.push(json!({"index":index,"clicks":clicks.get(),"feedback":feedback}));
    }
    std::fs::write(
        output.join("result.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    println!("PASS real navigation dispatch: normal click1, cancelled clicks0, rendered-only feedback, alert/stale/reboot suppression");
    Ok(())
}
