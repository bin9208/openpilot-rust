use super::*;
impl Device {
    pub(super) fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        scroller.add(Box::new(info::Info::new(&context)?))?;
        scroller.add(Box::new(updater::Updater::new(context.clone(), canvas)?))?;
        scroller.add(Box::new(pair::Pair::new(context.clone(), canvas)?))?;
        for (text, page, icon, offroad) in [
            ("review\ntraining guide", Page::Training, "info", true),
            (
                "driver\ncamera preview",
                Page::DriverCamera,
                "cameras",
                true,
            ),
            ("terms &\nconditions", Page::Terms, "info", false),
            ("regulatory info", Page::Regulatory, "info", false),
        ] {
            let mut button = BigButton::new(text, |path, size| paint::texture(canvas, path, size))?;
            button.icon = Some(paint::texture(
                canvas,
                &format!("icons_mici/settings/device/{icon}.png"),
                (64, 64),
            )?);
            let ctx = context.clone();
            button.state.click = Some(Box::new(move || ctx.open(page)));
            if offroad {
                let ui = context.ui.clone();
                button.state.enabled = Property::Dynamic(Box::new(move || !ui.borrow().started));
            }
            scroller.add(Box::new(button))?;
        }
        for (text, operation, icon, size) in [
            ("reset calibration", Operation::Reset, "lkas", (122, 64)),
            (
                "uninstall openpilot",
                Operation::Uninstall,
                "uninstall",
                (64, 64),
            ),
        ] {
            let icon = paint::texture(
                canvas,
                &format!("icons_mici/settings/device/{icon}.png"),
                size,
            )?;
            let mut button = BigButton::new(text, |path, size| paint::texture(canvas, path, size))?;
            button.icon = Some(icon);
            let ctx = context.clone();
            button.state.click = Some(Box::new(move || engaged(ctx.clone(), operation, icon)));
            scroller.add(Box::new(button))?;
        }
        for (operation, icon, size) in [
            (Operation::Reboot, "reboot", (64, 70)),
            (Operation::Shutdown, "power", (64, 66)),
        ] {
            let icon = paint::texture(
                canvas,
                &format!("icons_mici/settings/device/{icon}.png"),
                size,
            )?;
            let mut button =
                CircleButton::new(icon, |path, size| paint::texture(canvas, path, size))?;
            button.red = matches!(operation, Operation::Shutdown);
            if button.red {
                let ui = context.ui.clone();
                button.state.visible = Property::Dynamic(Box::new(move || !ui.borrow().ignition));
            }
            let ctx = context.clone();
            button.state.click = Some(Box::new(move || engaged(ctx.clone(), operation, icon)));
            scroller.add(Box::new(button))?;
        }
        Ok(Self {
            state: WidgetState::default(),
            scroller,
        })
    }
}
