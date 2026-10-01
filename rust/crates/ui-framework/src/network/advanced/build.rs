use super::*;
impl AdvancedNetworkSettings {
    pub fn new(
        context: Context,
        params: Rc<openpilot_params::Params>,
        mut texture: impl FnMut(&str, (i32, i32)) -> Result<Texture, Error>,
    ) -> Result<Self, Error> {
        let mut keyboard = Keyboard::new(
            KeyboardOptions {
                max_length: 64,
                min_length: 8,
                password_toggle: true,
                ..Default::default()
            },
            &mut texture,
        )?;
        keyboard.set_cancel_text(&context.text("Cancel"));
        let tether_enabled = Rc::new(Cell::new(true));
        let password_enabled = Rc::new(Cell::new(true));
        let metered_enabled = Rc::new(Cell::new(true));
        let actions = Rc::new(RefCell::new(VecDeque::new()));
        let mut scroller = Scroller {
            line_separator: true,
            spacing: 0.0,
            ..Default::default()
        };
        for (index, title) in [
            "Enable Tethering",
            "Tethering Password",
            "IP Address",
            "Enable Roaming",
            "APN Setting",
            "Cellular Metered",
            "Wi-Fi Network Metered",
            "Hidden Network",
        ]
        .into_iter()
        .enumerate()
        {
            let mut item = ListItem::new("")?;
            let translator = context.translate.clone();
            item.title = Property::Dynamic(Box::new(move || translator(title)));
            item.action = Some(match index {
                0 => {
                    let mut action = ToggleAction::new(false);
                    let flag = tether_enabled.clone();
                    action.state.enabled = Property::Dynamic(Box::new(move || flag.get()));
                    Box::new(action)
                }
                1 | 4 | 7 => {
                    let mut action = ButtonAction::new("");
                    let translator = context.translate.clone();
                    action.text = Property::Dynamic(Box::new(move || {
                        translator(if index == 7 { "CONNECT" } else { "EDIT" })
                    }));
                    if index == 1 {
                        let flag = password_enabled.clone();
                        action.state.enabled = Property::Dynamic(Box::new(move || flag.get()));
                    }
                    Box::new(action)
                }
                2 => {
                    let mut action = TextAction::new("", u32::from_le_bytes([170, 170, 170, 255]));
                    let session = context.session.clone();
                    action.text =
                        Property::Dynamic(Box::new(move || session.snapshot().ipv4_address));
                    Box::new(action)
                }
                3 => Box::new(ToggleAction::new(
                    params
                        .get_bool("GsmRoaming")
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?,
                )),
                5 => Box::new(ToggleAction::new(
                    params
                        .get_bool("GsmMetered")
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?,
                )),
                6 => {
                    let labels = ["default", "metered", "unmetered"]
                        .into_iter()
                        .map(|label| {
                            let tr = context.translate.clone();
                            Property::Dynamic(Box::new(move || tr(label)))
                        })
                        .collect();
                    let mut action = MultipleButtonAction::new(labels, 255.0, 0);
                    let flag = metered_enabled.clone();
                    action.state.enabled = Property::Dynamic(Box::new(move || flag.get()));
                    let queue = actions.clone();
                    action.callback = Some(Callback::new(move |selected| {
                        queue.borrow_mut().push_back(Action::WifiMetered(selected))
                    }));
                    Box::new(action)
                }
                _ => return Err(Error::Contract("invalid advanced network item")),
            });
            if matches!(index, 5 | 6) {
                let tr = context.translate.clone();
                item.description = Property::Dynamic(Box::new(move || {
                    tr(if index == 5 {
                        "Prevent large data uploads when on a metered cellular connection"
                    } else {
                        "Prevent large data uploads when on a metered Wi-Fi connection"
                    })
                }));
            }
            let action = match index {
                0 => Some(Action::Tether),
                1 => Some(Action::Password),
                3 => Some(Action::Roaming),
                4 => Some(Action::Apn),
                5 => Some(Action::CellMetered),
                7 => Some(Action::Hidden),
                _ => None,
            };
            if let Some(action) = action {
                let queue = actions.clone();
                item.callback = Some(Callback::new(move |()| {
                    queue.borrow_mut().push_back(action)
                }));
            }
            scroller.add(Box::new(item));
        }
        let events = context.session.subscribe();
        Ok(Self {
            state: WidgetState::default(),
            show_cell_settings: true.into(),
            keyboard: WidgetHandle::new(keyboard),
            scroller,
            context,
            params,
            tether_enabled,
            password_enabled,
            metered_enabled,
            errors: Rc::new(RefCell::new(Vec::new())),
            events,
            actions,
        })
    }
}
