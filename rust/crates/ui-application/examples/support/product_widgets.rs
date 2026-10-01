use super::Scene;
use openpilot_ui_application::{
    context::Context,
    widgets::{carrot_web::CarrotWeb, prime::PrimeWidget, setup::SetupWidget},
};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use std::{cell::RefCell, rc::Rc};
pub type DialogResults = Rc<RefCell<Vec<String>>>;
pub struct Product {
    pub widget: WidgetHandle,
    pub dialogs: DialogResults,
    pub network: Option<super::product_network::Fixture>,
    pub egpu: Option<std::sync::Arc<super::product_egpu::Fixture>>,
}
pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    scene: &Scene,
) -> Result<Product, Box<dyn std::error::Error>> {
    if scene.kind == "settings-root" {
        return super::product_settings::create(context, canvas, scene);
    }
    let dialog_results = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    if ["network-mici", "wifi-mici"].contains(&scene.kind.as_str()) {
        let (widget, network) = super::product_network::Fixture::create(context, canvas, scene)?;
        return Ok(Product {
            widget,
            dialogs: dialog_results,
            network: Some(network),
            egpu: None,
        });
    }
    if scene.kind == "egpu" {
        let (widget, egpu) = super::product_egpu::Fixture::create(context, canvas, scene)?;
        return Ok(Product {
            widget,
            dialogs: dialog_results,
            network: None,
            egpu: Some(egpu),
        });
    }
    let widget = if let Some(alert) = &scene.alert {
        super::product_alert::create(context, canvas, scene.config.big, alert)?
    } else if scene.camera.is_some() {
        super::product_camera::create(context, canvas, scene)?
    } else if let Some(options) = &scene.dialog {
        use openpilot_ui_application::mici::widgets::dialog;
        let results = dialog_results.clone();
        WidgetHandle::new(match scene.kind.as_str() {
            "dialog-info" => dialog::information(canvas, &options.title, &options.description)?,
            "dialog-confirm" => {
                let icon = openpilot_ui_application::paint::texture(
                    canvas,
                    "icons_mici/settings/device/reboot.png",
                    (64, 64),
                )?;
                dialog::confirmation(
                    canvas,
                    dialog::Confirmation {
                        title: options.title.clone(),
                        icon,
                        red: options.red,
                        exit_on_confirm: !options.stay,
                        callback: Rc::new(move || results.borrow_mut().push("confirm".into())),
                    },
                )?
            }
            "dialog-input" => dialog::InputDialog::create(
                canvas,
                dialog::InputOptions {
                    hint: options.title.clone(),
                    text: options.text.clone(),
                    minimum_length: 1,
                    auto_return: String::new(),
                    callback: Some(Rc::new(move |text| results.borrow_mut().push(text))),
                },
            )?,
            _ => return Err("unknown dialog".into()),
        })
    } else if scene.kind == "software" {
        openpilot_ui_application::settings::software::Software::create(context.clone())?
    } else if scene.kind == "device" {
        if scene.config.big {
            openpilot_ui_application::settings::device::Device::create(context.clone())?
        } else {
            openpilot_ui_application::mici::settings::device::Device::create(
                context.clone(),
                canvas,
            )?
        }
    } else if scene.kind == "toggles" {
        if scene.config.big {
            openpilot_ui_application::settings::toggles::Toggles::create(context.clone(), canvas)?
        } else {
            openpilot_ui_application::mici::settings::toggles::Toggles::create(
                context.clone(),
                canvas,
            )?
        }
    } else {
        WidgetHandle::from_box(match scene.kind.as_str() {
            "developer" => {
                if scene.config.big {
                    Box::new(
                        openpilot_ui_application::settings::developer::Developer::new(
                            context.clone(),
                            canvas,
                        )?,
                    )
                } else {
                    Box::new(
                        openpilot_ui_application::mici::settings::developer::Developer::new(
                            context.clone(),
                            canvas,
                        )?
                        .navigation(),
                    )
                }
            }
            "language" => Box::new(openpilot_ui_application::widgets::language::dialog(
                context.clone(),
                None,
            )?),
            "regulatory" => {
                let widget = openpilot_ui_application::widgets::regulatory::Regulatory::new(
                    context, canvas,
                )?;
                if scene.config.big {
                    Box::new(widget)
                } else {
                    Box::new(widget.navigation())
                }
            }
            "firehose" => {
                let widget =
                    openpilot_ui_application::widgets::firehose::Firehose::new(context.clone())?;
                if scene.config.big {
                    Box::new(widget)
                } else {
                    Box::new(widget.navigation(canvas))
                }
            }
            "ssh" => Box::new(openpilot_ui_application::widgets::ssh::SshAction::new(
                context.clone(),
                canvas,
            )?),
            "prime" => Box::new(PrimeWidget::new(context.clone())),
            "setup" => Box::new(SetupWidget::new(context.clone())),
            "pairing" if !scene.config.big => {
                let mut widget = openpilot_ui_application::mici::widgets::pairing::Pairing::new(
                    context.clone(),
                    canvas,
                )?;
                widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
                Box::new(widget.navigation(context.clone(), canvas))
            }
            "pairing" => {
                let mut widget = openpilot_ui_application::widgets::pairing::Pairing::new(
                    context.clone(),
                    canvas,
                )?;
                widget.url = Box::new(|| "https://connect.comma.ai/?pair=fixture".into());
                Box::new(widget)
            }
            "carrot-web" => Box::new(CarrotWeb::new(context.clone())),
            _ => return Err("unknown product kind".into()),
        })
    };
    Ok(Product {
        widget,
        dialogs: dialog_results,
        network: None,
        egpu: None,
    })
}
