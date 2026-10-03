use openpilot_ui_application::{layouts::training::Training, mici, widgets};
use openpilot_ui_framework::{
    dialog::{ConfirmDialog, MultiOptionDialog},
    navigation::NavWidget,
    slider::Slider,
    widget::WidgetHandle,
    Error,
};
pub fn active(widget: Option<&WidgetHandle>) -> Result<serde_json::Value, Error> {
    let Some(widget) = widget else {
        return Ok(serde_json::Value::Null);
    };
    if let Ok(nav) = widget.get::<NavWidget>() {
        let content = nav.content.as_ref() as &dyn std::any::Any;
        let mut scroll = None;
        let mut items = Vec::new();
        let kind = if let Some(settings) =
            content.downcast_ref::<mici::settings::layout::Settings>()
        {
            scroll = Some(settings.scroller.panel.offset());
            for index in 0..settings.scroller.len() {
                if let Some(item) = settings.scroller.item(index) {
                    let state = item.state();
                    items.push(serde_json::json!({"x":state.rect.x,"y":state.rect.y,"width":state.rect.width,"height":state.rect.height,"visible":state.visible.get()}));
                }
            }
            "settings"
        } else if content.is::<mici::settings::device::Device>() {
            "device"
        } else if content.is::<mici::settings::network::Network>() {
            "network"
        } else if content.is::<mici::settings::toggles::Toggles>() {
            "toggles"
        } else if content.is::<mici::settings::developer::Developer>() {
            "developer"
        } else if content.is::<mici::settings::egpu::Egpu>() {
            "egpu"
        } else if content.is::<widgets::firehose::Firehose>() {
            "firehose"
        } else if content.is::<widgets::regulatory::Regulatory>() {
            "regulatory"
        } else if content.is::<mici::widgets::pairing::Pairing>() {
            "pairing"
        } else if content.is::<mici::onroad::driver_camera::dialog::Dialog>() {
            "driver"
        } else if content.is::<mici::widgets::dialog::InputDialog>() {
            "input"
        } else if content.is::<Slider>() {
            "slider"
        } else if content.is::<mici::layouts::cards::Cards>() {
            "cards"
        } else {
            "navigation"
        };
        return Ok(
            serde_json::json!({"kind":kind,"scroll":scroll,"y":nav.state.rect.y,"items":items}),
        );
    }
    let kind = if widget.get::<ConfirmDialog>().is_ok() {
        "confirm"
    } else if widget.get::<MultiOptionDialog>().is_ok() {
        "select"
    } else if widget.get::<widgets::pairing::Pairing>().is_ok() {
        "pairing"
    } else if widget.get::<widgets::carrot_web::CarrotWeb>().is_ok() {
        "web"
    } else if widget
        .get::<openpilot_ui_application::onroad::driver_camera::Dialog>()
        .is_ok()
    {
        "driver"
    } else if widget.get::<widgets::regulatory::Regulatory>().is_ok() {
        "regulatory"
    } else if widget.get::<Training>().is_ok() {
        "training"
    } else {
        "root"
    };
    Ok(serde_json::json!({"kind":kind,"scroll":null,"y":null,"items":[]}))
}
