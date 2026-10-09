use super::{
    context::{Context, Reply},
    policy::{self, Action},
    runner::Failure,
};
use crate::Value;

pub(super) async fn run(context: &mut Context, body: &Value) -> Result<Reply, Failure> {
    if policy::needs_repo_lock(body.get("action"), body)? {
        if let Err(reply) = context.lock().await {
            return Ok(reply);
        }
    }
    let action = match Action::parse(body.get("action")) {
        Ok(action) => action,
        Err(value) => return Ok(Reply { status: 400, value }),
    };
    match action {
        Action::Branches => super::git_branch::list(context).await,
        Action::Checkout => super::git_checkout::checkout(context, body).await,
        Action::Log => super::git_log::run(context, body).await,
        Action::Pull => super::git_pull::run(context).await,
        Action::RemoteAdd => super::git_remote::change(context, body, true).await,
        Action::RemoteSet => super::git_remote::change(context, body, false).await,
        Action::Reset => super::git_reset::reset(context, body).await,
        Action::Sync => super::git_reset::sync(context).await,
        Action::FactoryFetch => super::git_factory::fetch(context).await,
        Action::FactoryCheckout => super::git_factory_checkout::checkout(context, body).await,
        Action::DeleteLogs => super::files::delete(context, true),
        Action::DeleteVideos => super::files::delete(context, false),
        Action::Backup => super::files::backup(context),
        Action::SendTmux => super::files::send_tmux(context),
        Action::Calibration | Action::CaptureTmux | Action::Reboot | Action::Rebuild => {
            super::system_actions::run(context, action).await
        }
        Action::Shell => super::shell_action::run(context, body).await,
    }
}
