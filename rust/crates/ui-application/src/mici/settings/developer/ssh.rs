use super::*;
use crate::{
    mici::widgets::{big_button::BigButton, dialog::InputOptions},
    params::Read,
};
use openpilot_ui_framework::{callback::Callback, geometry::Rect};
use std::cell::{Cell, RefCell};
struct Model {
    value: RefCell<String>,
    enabled: Cell<bool>,
}
pub(super) struct Ssh {
    button: BigButton,
    model: Rc<Model>,
}
impl Ssh {
    pub fn new(
        policy: &Rc<Policy>,
        fetcher: &Rc<RefCell<Fetcher>>,
        canvas: &mut Canvas,
    ) -> Result<Self, Error> {
        let username = policy.context.params.string("GithubUsername")?;
        let model = Rc::new(Model {
            value: RefCell::new(if username.is_empty() {
                "Not set".into()
            } else {
                username
            }),
            enabled: Cell::new(true),
        });
        let mut button =
            BigButton::new("SSH keys", |path, size| paint::texture(canvas, path, size))?;
        button.icon = Some(paint::texture(
            canvas,
            "icons_mici/settings/developer/ssh.png",
            (56, 64),
        )?);
        let state = model.clone();
        button.state.enabled = Property::Dynamic(Box::new(move || state.enabled.get()));
        let weak_policy = Rc::downgrade(policy);
        let weak_fetcher = Rc::downgrade(fetcher);
        let weak_model = Rc::downgrade(&model);
        button.state.click = Some(Box::new(move || {
            let Some(policy) = weak_policy.upgrade() else {
                return;
            };
            let result = (|| -> Result<(), crate::Error> {
                if !policy.context.system_time_valid()? {
                    policy.context.actions.push(Action::MiciAlert {
                        title: String::new(),
                        description: "Please connect to Wi-Fi to fetch your key.".into(),
                    });
                    return Ok(());
                }
                let username = policy.context.params.string("GithubUsername")?;
                let callback_policy = weak_policy.clone();
                let callback_fetcher = weak_fetcher.clone();
                let callback_model = weak_model.clone();
                let callback = Rc::new(move |username: String| {
                    let (Some(policy), Some(fetcher), Some(model)) = (
                        callback_policy.upgrade(),
                        callback_fetcher.upgrade(),
                        callback_model.upgrade(),
                    ) else {
                        return;
                    };
                    let result = (|| -> Result<(), crate::Error> {
                        if username.is_empty() {
                            fetcher
                                .borrow()
                                .clear()
                                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?;
                            *model.value.borrow_mut() = "Not set".into();
                        } else {
                            *model.value.borrow_mut() = "Loading...".into();
                            model.enabled.set(false);
                            let policy = Rc::downgrade(&policy);
                            let model = Rc::downgrade(&model);
                            let display = username.clone();
                            let response = Callback::new(move |error: Option<String>| {
                                let (Some(policy), Some(model)) =
                                    (policy.upgrade(), model.upgrade())
                                else {
                                    return;
                                };
                                model.enabled.set(true);
                                if let Some(error) = error {
                                    *model.value.borrow_mut() = "Not set".into();
                                    policy.context.actions.push(Action::MiciAlert {
                                        title: String::new(),
                                        description: error,
                                    });
                                } else {
                                    *model.value.borrow_mut() = display.clone();
                                }
                            });
                            fetcher.borrow_mut().fetch(username, response)?;
                        }
                        Ok(())
                    })();
                    if let Err(error) = result {
                        policy.context.actions.push(Action::Failure(error));
                    }
                });
                policy.context.actions.push(Action::MiciInput(InputOptions {
                    hint: "enter GitHub username...".into(),
                    text: username,
                    minimum_length: 0,
                    callback: Some(callback),
                    auto_return: String::new(),
                }));
                Ok(())
            })();
            if let Err(error) = result {
                policy.context.actions.push(Action::Failure(error));
            }
        }));
        Ok(Self { button, model })
    }
}
impl Widget for Ssh {
    fn state(&self) -> &WidgetState {
        &self.button.state
    }
    fn state_mut(&mut self) -> &mut WidgetState {
        &mut self.button.state
    }
    fn set_position(&mut self, x: f32, y: f32) {
        self.button.set_position(x, y);
    }
    fn set_rect(&mut self, rect: Rect) {
        self.button.set_rect(rect);
    }
    fn paint(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.paint(frame, draw)
    }
    fn render(&mut self, frame: &Frame<'_>, draw: &mut dyn Draw) -> Result<RenderResult, Error> {
        self.button.value = self.model.value.borrow().clone();
        self.button.render(frame, draw)
    }
}
