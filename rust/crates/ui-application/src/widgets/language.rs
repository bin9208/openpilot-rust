use crate::context::{Action, Context};
use openpilot_ui_framework::{
    callback::Callback, dialog::MultiOptionDialog, text::Font, widget::DialogResult, Error,
};
pub fn dialog(context: Context, changed: Option<Callback<()>>) -> Result<MultiOptionDialog, Error> {
    let (options, current) = context.translations.update(|translations| {
        (
            translations.language_order.clone(),
            translations.codes.get(translations.language()).cloned(),
        )
    });
    let current = current.ok_or(Error::Contract("current language missing from catalog"))?;
    let mut dialog = MultiOptionDialog::new(&context.tr("Select a language"), options, &current);
    dialog.set_option_font(Font::Unifont);
    dialog.cancel.label.text = context.text("Cancel");
    dialog.select.label.text = context.text("Select");
    let selection = dialog.selection.clone();
    dialog.callback = Some(Callback::new(move |result| {
        if result != DialogResult::Confirm {
            return;
        }
        let result = (|| -> Result<(), crate::Error> {
            let code = context
                .translations
                .update(|translations| translations.languages.get(&*selection.borrow()).cloned())
                .ok_or(crate::Error::Contract(
                    "selected language missing from catalog",
                ))?;
            context
                .translations
                .update(|translations| translations.change_language(&code, &context.params.raw))
                .map_err(|error| crate::Error::Io(std::io::Error::other(error)))?;
            context.actions.push(Action::SetLanguage(code));
            if let Some(callback) = &changed {
                callback.call(());
            }
            Ok(())
        })();
        if let Err(error) = result {
            context.actions.push(Action::Failure(error));
        }
    }));
    Ok(dialog)
}
