use super::*;
use crate::context::{actions::UpdaterAction, Action, Page};
use num_traits::ToPrimitive;
use openpilot_ui_framework::{
    canvas::Canvas,
    dialog::{ConfirmDialog, MultiOptionDialog},
    widget::{DialogResult, Frame},
};
pub(super) struct Effects {
    context: Context,
    updater: PathBuf,
    camera: String,
    publisher: openpilot_messaging::runtime::PubMaster,
    pub show_touches: bool,
    pub show_fps: bool,
    training: Option<WidgetHandle>,
}
impl Effects {
    pub fn new(
        context: Context,
        updater: PathBuf,
        camera: String,
        show_touches: bool,
        show_fps: bool,
    ) -> Result<Self, Error> {
        Ok(Self {
            context,
            updater,
            camera,
            publisher: openpilot_messaging::runtime::PubMaster::for_runtime(&["bookmarkButton"])
                .map_err(|error| Error::Io(std::io::Error::other(error)))?,
            show_touches,
            show_fps,
            training: None,
        })
    }
    fn push(frame: &Frame<'_>, widget: impl openpilot_ui_framework::widget::Widget) {
        frame
            .navigation
            .push(NavigationRequest::Push(WidgetHandle::new(widget)));
    }
    fn page(
        &mut self,
        page: Page,
        context: &Context,
        root: &WidgetHandle,
        frame: &Frame<'_>,
        canvas: &mut Canvas,
        stack: &openpilot_ui_framework::stack::NavigationStack,
    ) -> Result<(), Error> {
        if root.get_mut::<Main>()?.open(page, frame)? {
            return Ok(());
        }
        match page {
            Page::CarrotWeb => Self::push(
                frame,
                crate::widgets::carrot_web::CarrotWeb::new(context.clone()),
            ),
            Page::Pairing => {
                if context.big {
                    Self::push(
                        frame,
                        crate::widgets::pairing::Pairing::new(context.clone(), canvas)?,
                    );
                } else {
                    Self::push(
                        frame,
                        crate::mici::widgets::pairing::Pairing::new(context.clone(), canvas)?
                            .navigation(context.clone(), canvas),
                    );
                }
            }
            Page::DriverCamera => {
                if context.big {
                    Self::push(
                        frame,
                        crate::onroad::driver_camera::Dialog::with_camera(
                            context.clone(),
                            canvas,
                            &self.camera,
                        )?,
                    );
                } else {
                    Self::push(
                        frame,
                        crate::mici::onroad::driver_camera::dialog::with_camera(
                            context.clone(),
                            canvas,
                            &self.camera,
                        )?,
                    );
                }
            }
            Page::Language => Self::push(
                frame,
                crate::widgets::language::dialog(context.clone(), None)?,
            ),
            Page::Regulatory => {
                let regulatory = crate::widgets::regulatory::Regulatory::new(context, canvas)?;
                if context.big {
                    Self::push(frame, regulatory);
                } else {
                    Self::push(frame, regulatory.navigation());
                }
            }
            Page::Training => {
                if context.big {
                    if self.training.is_none() {
                        self.training = Some(WidgetHandle::new(
                            crate::layouts::training::Training::new(context.clone(), canvas)?,
                        ));
                    }
                    frame.navigation.push(NavigationRequest::Push(
                        self.training
                            .as_ref()
                            .ok_or(Error::Contract("review training missing"))?
                            .clone(),
                    ));
                } else {
                    let target = stack
                        .active()
                        .ok_or(Error::Contract("review training parent missing"))?;
                    let queue = frame.navigation.clone();
                    let completed = Rc::new(move || {
                        queue.push(NavigationRequest::PopTo {
                            target: target.clone(),
                            instant: false,
                            callback: None,
                        })
                    });
                    Self::push(
                        frame,
                        crate::mici::layouts::onboarding::review_training(
                            context.clone(),
                            canvas,
                            completed,
                            &self.camera,
                        )?,
                    );
                }
            }
            Page::Terms => Self::push(
                frame,
                crate::mici::layouts::cards::review_terms(context.clone(), canvas)?,
            ),
            Page::Home | Page::Settings(_) => {
                return Err(Error::Contract("root page action was not handled"));
            }
        }
        Ok(())
    }
    pub fn drain(
        &mut self,
        root: &WidgetHandle,
        frame: &Frame<'_>,
        canvas: &mut Canvas,
        diagnostics: &mut openpilot_startup_ui::diagnostics::Diagnostics,
        stack: &mut openpilot_ui_framework::stack::NavigationStack,
        hardware: &mut dyn hardware::Hardware,
    ) -> Result<(), Error> {
        let context = self.context.clone();
        let context = &context;
        while let Some(action) = context.actions.pop() {
            match action {
                Action::Open(page) => self.page(page, context, root, frame, canvas, stack)?,
                Action::Select(selection) => {
                    let mut dialog = MultiOptionDialog::new(
                        &selection.title,
                        selection.options,
                        &selection.selected,
                    );
                    let value = dialog.selection.clone();
                    dialog.callback = Some(Callback::new(move |result| {
                        selection
                            .callback
                            .call((result == DialogResult::Confirm).then(|| value.borrow().clone()))
                    }));
                    Self::push(frame, dialog);
                }
                Action::Confirm(options) => {
                    let mut dialog =
                        ConfirmDialog::new(&options.text, &options.confirm, &options.cancel)?;
                    dialog.rich = options.rich;
                    dialog.callback = Some(options.callback);
                    Self::push(frame, dialog);
                }
                Action::Alert(text) => {
                    Self::push(frame, ConfirmDialog::new(&text, &context.tr("OK"), "")?)
                }
                Action::MiciAlert { title, description } => Self::push(
                    frame,
                    crate::mici::widgets::dialog::information(canvas, &title, &description)?,
                ),
                Action::MiciConfirm(options) => Self::push(
                    frame,
                    crate::mici::widgets::dialog::confirmation(canvas, options)?,
                ),
                Action::MiciInput(options) => Self::push(
                    frame,
                    crate::mici::widgets::dialog::InputDialog::create(canvas, options)?,
                ),
                Action::Failure(error) => return Err(error.into()),
                Action::PairingCheck => {
                    if !context.prime.is_paired() {
                        self.page(Page::Pairing, context, root, frame, canvas, stack)?;
                    }
                }
                Action::Updater(UpdaterAction::Reboot) => hardware.reboot()?,
                Action::Updater(action @ (UpdaterAction::Check | UpdaterAction::Download)) => {
                    let sm = context.messages.borrow();
                    let manager = messages::manager_state(&sm.state)?;
                    let request = match action {
                        UpdaterAction::Check => crate::services::updater::Request::Check,
                        UpdaterAction::Download => crate::services::updater::Request::Download,
                        UpdaterAction::Reboot => {
                            return Err(Error::Contract("reboot already handled"));
                        }
                    };
                    let outcome = crate::services::updater::send(manager, &self.updater, request)
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                    if let crate::services::updater::Outcome::Unavailable(reason) = outcome {
                        openpilot_startup_ui::logging::emit(
                            openpilot_logging::record::Level::Warning,
                            format!("UI updater unavailable: {reason:?}"),
                        );
                    }
                }
                Action::Recording(value) => {
                    if value {
                        diagnostics.start_recording(&mut canvas.renderer)?;
                    } else {
                        diagnostics.stop_recording()?;
                    }
                }
                Action::ToggleRecording => diagnostics.toggle_recording(&mut canvas.renderer)?,
                Action::Bookmark => {
                    let mut message = capnp::message::Builder::new_default();
                    let mut event =
                        message.init_root::<openpilot_cereal::log_capnp::event::Builder>();
                    event.set_valid(true);
                    event.set_log_mono_time(
                        ((context.now_monotonic)() * 1e9)
                            .to_u64()
                            .ok_or(Error::Contract("invalid bookmark timestamp"))?,
                    );
                    event.init_bookmark_button();
                    self.publisher
                        .send(
                            "bookmarkButton",
                            &capnp::serialize::write_message_to_words(&message),
                        )
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                }
                Action::SetOffroadBrightness(value) => {
                    context.device.borrow_mut().set_offroad_brightness(value)
                }
                Action::SetInteractiveTimeout(value) => {
                    context.device.borrow_mut().set_override_timeout(
                        value,
                        (context.now_monotonic)(),
                        context.ui.borrow().ignition,
                    )
                }
                Action::RefreshParams => context.refresh_params()?,
                Action::ShowTouches(value) => {
                    self.show_touches = value;
                    diagnostics.options.show_touches = value;
                }
                Action::ShowFps(value) => {
                    self.show_fps = value;
                    diagnostics.options.show_fps = value;
                }
                Action::SetLanguage(code) => {
                    context
                        .translations
                        .update(|translations| {
                            if translations.language() != code {
                                translations.change_language(&code, &context.params.raw)
                            } else {
                                Ok(())
                            }
                        })
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                    canvas.renderer.set_language(&code);
                }
                Action::SelectLanguage(changed) => Self::push(
                    frame,
                    crate::widgets::language::dialog(context.clone(), Some(changed))?,
                ),
                Action::Exit => frame.navigation.push(NavigationRequest::Close),
            }
            stack.process(frame)?;
        }
        Ok(())
    }
}
