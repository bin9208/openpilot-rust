use super::{product_egpu, product_network, product_widgets::Product, Scene};
use openpilot_ui_application::{
    context::{Context, Page},
    settings::resources::{Network, Resources},
};
use openpilot_ui_framework::{callback::Callback, canvas::Canvas, widget::WidgetHandle};
use std::{cell::RefCell, rc::Rc};

pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    scene: &Scene,
) -> Result<Product, Box<dyn std::error::Error>> {
    let (session, network) = product_network::Fixture::new(scene)?;
    let egpu = product_egpu::Fixture::new(scene)?;
    let resources = Resources {
        network: Network {
            session,
            params: Rc::new(context.params.raw.as_ref().clone()),
        },
        ticks: network.ticks.clone(),
        egpu: egpu.clone(),
    };
    let widget = if scene.config.big {
        let mut settings = openpilot_ui_application::settings::layout::Settings::new(
            context.clone(),
            canvas,
            resources,
        )?;
        let ctx = context.clone();
        settings.on_close = Some(Callback::new(move |()| ctx.open(Page::Home)));
        WidgetHandle::new(settings)
    } else {
        WidgetHandle::new(
            openpilot_ui_application::mici::settings::layout::Settings::new(
                context.clone(),
                canvas,
                resources,
            )?
            .navigation(),
        )
    };
    Ok(Product {
        widget,
        dialogs: Rc::new(RefCell::new(Vec::new())),
        network: Some(network),
        egpu: Some(egpu),
    })
}

pub fn page(widget: &WidgetHandle) -> Result<Option<&'static str>, openpilot_ui_framework::Error> {
    let widget = widget.borrow()?;
    let Some(nav) = (widget.as_ref() as &dyn std::any::Any)
        .downcast_ref::<openpilot_ui_framework::navigation::NavWidget>()
    else {
        return Ok(None);
    };
    let content = nav.content.as_ref() as &dyn std::any::Any;
    use openpilot_ui_application::{mici::settings, widgets::firehose::Firehose};
    Ok(if content.is::<settings::device::Device>() {
        Some("Settings(Device)")
    } else if content.is::<settings::toggles::Toggles>() {
        Some("Settings(Toggles)")
    } else if content.is::<settings::network::Network>() {
        Some("Settings(Network)")
    } else if content.is::<settings::developer::Developer>() {
        Some("Settings(Developer)")
    } else if content.is::<settings::egpu::Egpu>() {
        Some("Settings(Egpu)")
    } else if content.is::<Firehose>() {
        Some("Settings(Firehose)")
    } else {
        None
    })
}
