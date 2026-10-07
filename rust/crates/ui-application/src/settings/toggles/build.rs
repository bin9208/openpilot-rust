use super::*;
use crate::{
    paint,
    params::{typed, Read},
};
impl Toggles {
    pub(super) fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let is_release = context.params.boolean("IsReleaseBranch")?;
        let changes = Rc::new(RefCell::new(VecDeque::new()));
        let mut scroller = Scroller {
            spacing: 0.0,
            line_separator: true,
            ..Default::default()
        };
        let mut indices = BTreeMap::new();
        let mut locked = BTreeSet::new();
        let mut personality = ListItem::new("")?;
        personality.title = context.text("Driving Personality");
        personality.description = context.text(copy::PERSONALITY);
        personality.icon = Some(paint::texture(canvas, "icons/speed_limit.png", (80, 80))?);
        let selected =
            typed::integer_value(context.params.as_ref(), "LongitudinalPersonality", true)?
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(usize::MAX);
        let mut personality_action = MultipleButtonAction::new(
            ["Aggressive", "Standard", "Relaxed", "moreRelaxed"]
                .into_iter()
                .map(|text| context.text(text))
                .collect(),
            255.0,
            selected,
        );
        let queue = changes.clone();
        personality_action.callback = Some(Callback::new(move |value| {
            queue.borrow_mut().push_back(Change::Personality(value))
        }));
        personality.action = Some(Box::new(personality_action));
        let mut personality = Some(personality);
        for definition in copy::DEFINITIONS {
            let key = definition.key;
            let lock = format!("{key}Lock");
            let is_locked =
                openpilot_params::metadata(&lock).is_some() && context.params.boolean(&lock)?;
            let mut item = ListItem::new("")?;
            item.title = context.text(definition.title);
            item.icon = Some(paint::texture(
                canvas,
                &format!("icons/{}", definition.icon),
                (80, 80),
            )?);
            let mut action = ToggleAction::new(context.params.boolean(key)?);
            action.state.enabled = (!is_locked).into();
            let queue = changes.clone();
            action.toggle.changed = Some(Box::new(move |value| {
                queue.borrow_mut().push_back(Change::Toggle(key, value))
            }));
            item.action = Some(Box::new(action));
            let extra = if definition.restart && !is_locked {
                context.tr("Changing this setting will restart openpilot if the car is powered on.")
            } else {
                String::new()
            };
            let source = definition.description;
            let translations = context.translations.clone();
            item.description =
                openpilot_ui_framework::widget::Property::Dynamic(Box::new(move || {
                    let text = translations.tr(source);
                    if extra.is_empty() {
                        text
                    } else {
                        format!("{text} {}", translations.tr(&extra))
                    }
                }));
            if is_locked {
                locked.insert(key);
            }
            indices.insert(key, scroller.items.len());
            scroller.add(Box::new(item));
            if key == "DisengageOnAccelerator" {
                indices.insert("LongitudinalPersonality", scroller.items.len());
                scroller.add(Box::new(
                    personality
                        .take()
                        .ok_or(Error::Contract("personality already inserted"))?,
                ));
            }
        }
        let icons = [
            paint::texture(canvas, "icons/experimental_white.png", (80, 80))?,
            paint::texture(canvas, "icons/experimental.png", (80, 80))?,
        ];
        let mut widget = Self {
            state: WidgetState::default(),
            context,
            scroller,
            indices,
            locked,
            icons,
            changes,
            handle: None,
            is_release,
        };
        widget.update_icon()?;
        Ok(widget)
    }
}
