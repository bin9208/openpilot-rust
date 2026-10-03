use crate::{
    context::{Action, Context, Event},
    mici::widgets::big_button::{BigButton, Kind},
    paint,
    params::{binding::Binding, Read},
    state::messages,
};
use openpilot_ui_framework::{
    callback::Callback,
    canvas::Canvas,
    draw::Draw,
    navigation::NavWidget,
    scroller::Scroller,
    widget::{Frame, Property, RenderResult, Widget, WidgetHandle, WidgetState},
    Error,
};
use std::collections::BTreeMap;
pub struct Toggles {
    state: WidgetState,
    context: Context,
    pub scroller: Scroller,
    indices: BTreeMap<&'static str, usize>,
}
const DEFINITIONS: &[(&str, &str, bool)] = &[
    ("ExperimentalMode", "experimental mode", false),
    ("IsMetric", "use metric units", false),
    ("IsLdwEnabled", "lane departure warnings", false),
    ("AlwaysOnDM", "always-on driver monitor", false),
    ("RecordFront", "record & upload driver camera", true),
    ("RecordAudio", "record & upload mic audio", true),
    ("OpenpilotEnabledToggle", "enable openpilot", true),
];
impl Toggles {
    pub fn create(context: Context, canvas: &mut Canvas) -> Result<WidgetHandle, Error> {
        let content = Self::new(context.clone(), canvas)?;
        let widget = WidgetHandle::new(NavWidget::new(
            Box::new(content),
            20.0,
            f64::from(canvas.renderer.config.height()),
        ));
        let weak = widget.downgrade();
        let actions = context.actions.clone();
        context.listen(
            Event::Engaged,
            Callback::new(move |()| {
                let result = (|| -> Result<(), crate::Error> {
                    let Some(widget) = weak.upgrade() else {
                        return Ok(());
                    };
                    let mut nav = widget.get_mut::<NavWidget>()?;
                    let content = (nav.content.as_mut() as &mut dyn std::any::Any)
                        .downcast_mut::<Toggles>()
                        .ok_or(Error::Contract("Mici toggles content missing"))?;
                    content.refresh()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    actions.push(Action::Failure(error));
                }
            }),
        );
        Ok(widget)
    }
    fn new(context: Context, canvas: &mut Canvas) -> Result<Self, Error> {
        let mut scroller = Scroller::new(true, false, !context.pc, 20.0);
        scroller.indicator = Some(paint::texture(
            canvas,
            "icons_mici/settings/horizontal_scroll_indicator.png",
            (96, 48),
        )?);
        let mut personality = BigButton::new("driving personality", |path, size| {
            paint::texture(canvas, path, size)
        })?;
        personality.set_multiple(
            ["aggressive", "standard", "relaxed", "moreRelaxed"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )?;
        personality.binding = Some(Binding {
            params: context.params.clone(),
            key: "LongitudinalPersonality".into(),
            asynchronous: true,
        });
        personality.refresh_param()?;
        scroller.add(Box::new(personality))?;
        let mut indices = BTreeMap::from([("LongitudinalPersonality", 0)]);
        for (key, text, restart) in DEFINITIONS {
            let mut button = BigButton::new(text, |path, size| paint::texture(canvas, path, size))?;
            button.kind = Kind::Toggle(context.params.boolean(key)?);
            button.binding = Some(Binding {
                params: context.params.clone(),
                key: (*key).into(),
                asynchronous: false,
            });
            if *restart {
                let context = context.clone();
                button.changed = Some(Callback::new(move |_| {
                    if let Err(error) = context.params.put_bool("OnroadCycleRequested", true) {
                        context.actions.push(Action::Failure(error));
                    }
                }));
            }
            if matches!(
                *key,
                "OpenpilotEnabledToggle" | "RecordFront" | "RecordAudio"
            ) {
                if *key == "RecordFront" && context.params.boolean("RecordFrontLock")? {
                    button.state.enabled = false.into();
                } else {
                    let ui = context.ui.clone();
                    button.state.enabled =
                        Property::Dynamic(Box::new(move || !ui.borrow().engaged));
                }
            }
            indices.insert(*key, scroller.len());
            scroller.add(Box::new(button))?;
        }
        if context.params.boolean("ShowDebugInfo")? {
            context.actions.push(Action::ShowTouches(true));
            context.actions.push(Action::ShowFps(true));
        }
        Ok(Self {
            state: WidgetState::default(),
            context,
            scroller,
            indices,
        })
    }
    fn button(&mut self, key: &str) -> Result<&mut BigButton, Error> {
        let index = *self
            .indices
            .get(key)
            .ok_or(Error::Contract("Mici toggle missing"))?;
        self.scroller
            .item_mut(index)
            .and_then(|item| (item as &mut dyn std::any::Any).downcast_mut::<BigButton>())
            .ok_or(Error::Contract("Mici toggle type mismatch"))
    }
    pub fn refresh(&mut self) -> Result<(), Error> {
        self.context.refresh_params()?;
        let (car, longitudinal) = {
            let ui = self.context.ui.borrow();
            (ui.slow.car, ui.slow.has_longitudinal_control)
        };
        if car.is_some() {
            if longitudinal {
                self.button("ExperimentalMode")?.state.visible = true.into();
                self.button("LongitudinalPersonality")?.state.visible = true.into();
            } else {
                let button = self.button("ExperimentalMode")?;
                button.state.visible = false.into();
                button.set_checked(false);
                self.context.params.remove("ExperimentalMode")?;
            }
        }
        for (key, _, _) in DEFINITIONS {
            let value = self.context.params.boolean(key)?;
            self.button(key)?.set_checked(value);
        }
        Ok(())
    }
}
impl Widget for Toggles {
    fn state(&self) -> &WidgetState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.state
    }
    fn show(&mut self, frame: &Frame<'_>) {
        self.scroller.show(frame);
        if let Err(error) = self.refresh() {
            self.context.actions.push(Action::Failure(error.into()));
        }
    }
    fn update(&mut self, _: &Frame<'_>, _: &mut dyn Draw) -> Result<(), Error> {
        let value = {
            let messages = self.context.messages.borrow();
            if messages
                .state
                .topic("selfdriveState")
                .map_err(crate::Error::from)?
                .updated
            {
                Some(i32::from(u16::from(
                    messages::selfdrive_state(&messages.state)?
                        .get_personality()
                        .map_err(crate::Error::from)?,
                )))
            } else {
                None
            }
        };
        if let Some(value) = value {
            let change = {
                let ui = self.context.ui.borrow();
                ui.started && ui.personality != value
            };
            if change {
                let button = self.button("LongitudinalPersonality")?;
                let Kind::Multiple { options, .. } = &button.kind else {
                    return Err(Error::Contract("personality is not a multiple toggle"));
                };
                button.value = options
                    .get(
                        usize::try_from(value)
                            .map_err(|_| Error::Contract("negative personality"))?,
                    )
                    .ok_or(Error::Contract("personality out of range"))?
                    .clone();
            }
            self.context.ui.borrow_mut().personality = value;
        }
        Ok(())
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.scroller.state.enabled = self.state.enabled.get().into();
        self.scroller.set_rect(self.state.rect);
        self.scroller.render(frame, draw)
    }
}
