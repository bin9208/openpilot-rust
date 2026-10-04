use super::*;
use crate::params::Read;
use openpilot_ui_framework::{
    list::{ButtonAction, TextAction},
    widget::Property,
};
pub(super) const CALIBRATION: &str = "openpilot requires the device to be mounted within 4° left or right and within 5° up or 9° down.";
impl Device {
    pub(super) fn new(context: Context) -> Result<Self, Error> {
        let calibration = Rc::new(RefCell::new(None::<String>));
        let changes = Rc::new(RefCell::new(VecDeque::new()));
        let mut scroller = Scroller {
            spacing: 0.0,
            line_separator: true,
            ..Default::default()
        };
        for (title, key) in [("Dongle ID", "DongleId"), ("Serial", "HardwareSerial")] {
            let mut item = ListItem::new("")?;
            item.title = context.text(title);
            let text = context.params.string(key)?;
            let mut action = TextAction::new("", crate::paint::color(170, 170, 170, 255));
            action.text = if text.is_empty() {
                context.text("N/A")
            } else {
                text.into()
            };
            item.action = Some(Box::new(action));
            scroller.add(Box::new(item));
        }
        let definitions = [
            ("Pair Device","PAIR","Pair your device with comma connect (connect.comma.ai) and claim your comma prime offer.",Some(Page::Pairing),false),
            ("Driver Camera","PREVIEW","Preview the driver facing camera to ensure that driver monitoring has good visibility. (vehicle must be off)",Some(Page::DriverCamera),true),
            ("Reset Calibration","RESET",CALIBRATION,None,false),
            ("Review Training Guide","REVIEW","Review the rules, features, and limitations of openpilot",Some(Page::Training),true),
            ("Regulatory","VIEW","",Some(Page::Regulatory),true),
            ("Change Language","CHANGE","",None,false),
        ];
        for (title, button, description, page, offroad) in definitions {
            let mut item = ListItem::new("")?;
            item.title = context.text(title);
            item.description = context.text(description);
            let mut action = ButtonAction::new("");
            action.text = context.text(button);
            if offroad {
                let ui = context.ui.clone();
                action.state.enabled = Property::Dynamic(Box::new(move || !ui.borrow().started));
            }
            item.action = Some(Box::new(action));
            if title == "Pair Device" {
                let prime = context.prime.clone();
                item.state.visible = Property::Dynamic(Box::new(move || !prime.is_paired()));
            }
            let queue = changes.clone();
            let ctx = context.clone();
            item.callback = Some(Callback::new(move |()| {
                if let Some(page) = page {
                    ctx.open(page);
                } else {
                    queue
                        .borrow_mut()
                        .push_back(if title == "Reset Calibration" {
                            Change::Reset
                        } else {
                            Change::Language
                        });
                }
            }));
            if title == "Reset Calibration" {
                let current = calibration.clone();
                let translations = context.translations.clone();
                item.description = Property::Dynamic(Box::new(move || {
                    current
                        .borrow()
                        .clone()
                        .unwrap_or_else(|| translations.tr(CALIBRATION))
                }));
                let current = calibration.clone();
                let ctx = context.clone();
                item.description_opened =
                    Some(Callback::new(
                        move |()| match super::calibration::description(&ctx) {
                            Ok(value) => *current.borrow_mut() = Some(value),
                            Err(error) => ctx.actions.push(Action::Failure(error)),
                        },
                    ));
            }
            scroller.add(Box::new(item));
        }
        let mut power = ListItem::new("")?;
        let mut action = DualButtonAction::new("", "");
        action.left.label.text = context.text("Reboot");
        action.right.label.text = context.text("Power Off");
        let queue = changes.clone();
        action.left.state.click = Some(Box::new(move || {
            queue.borrow_mut().push_back(Change::Reboot)
        }));
        let queue = changes.clone();
        action.right.state.click = Some(Box::new(move || {
            queue.borrow_mut().push_back(Change::Shutdown)
        }));
        power.action = Some(Box::new(action));
        scroller.add(Box::new(power));
        Ok(Self {
            state: WidgetState::default(),
            context,
            scroller,
            changes,
            handle: None,
            calibration,
        })
    }
}
