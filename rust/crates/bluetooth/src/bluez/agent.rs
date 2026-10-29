use super::{
    integer::{self, Unsigned},
    peer::Peer,
    policy::PromptKind,
    state::{Prompt, Shared},
    wire, Error, AGENT,
};
use dbus::{channel::MatchingReceiver, channel::Sender, message::MatchRule, Message};
use openpilot_logmessaged::{JsonValue, JsonView};
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinSet,
};

pub(super) struct Server {
    stop: watch::Sender<bool>,
    tasks: JoinSet<Result<(), Error>>,
}

impl Server {
    pub fn start(peer: Peer, state: Shared) -> Result<Self, Error> {
        let (incoming, mut receiver) = mpsc::channel(16);
        let mut rule = MatchRule::new_method_call();
        rule.path = Some(dbus::Path::new(AGENT).map_err(Error::Request)?);
        let token = peer.connection.start_receive(
            rule,
            Box::new(move |message, _| match incoming.try_send(message) {
                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => true,
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            }),
        );
        let (stop, mut stopping) = watch::channel(false);
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            let mut replies = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = stopping.changed() => break,
                    joined = replies.join_next(), if !replies.is_empty() => {
                        if let Some(joined) = joined { joined??; }
                    }
                    message = receiver.recv() => {
                        let Some(message) = message else { break; };
                        let peer = peer.clone();
                        let state = state.clone();
                        let stopping = stopping.clone();
                        replies.spawn(async move {
                            let response = match reply(message, state, stopping).await {
                                Ok(response) => response,
                                Err(error) => { eprintln!("Bluetooth agent reply: {error}"); return Ok::<(), Error>(()); }
                            };
                            if let Some(response) = response {
                                if peer.connection.send(response).is_err() {
                                    eprintln!("Bluetooth agent reply connection closed");
                                }
                            }
                            Ok(())
                        });
                    }
                }
            }
            drop(peer.connection.stop_receive(token));
            while let Some(joined) = replies.join_next().await { joined??; }
            Ok(())
        });
        Ok(Self { stop, tasks })
    }

    pub async fn close(mut self) -> Result<(), Error> {
        self.stop.send_replace(true);
        while let Some(joined) = self.tasks.join_next().await {
            joined??;
        }
        Ok(())
    }
}

async fn reply(
    message: Message,
    shared: Shared,
    mut stopping: watch::Receiver<bool>,
) -> Result<Option<Message>, Error> {
    let method = message
        .member()
        .map(|value| value.to_string())
        .unwrap_or_default();
    {
        let mut state = shared.lock()?;
        if message.sender().as_deref() != state.owner.as_deref()
            || message.interface().as_deref() != Some("org.bluez.Agent1")
        {
            return wire::error(&message, "org.bluez.Error.Rejected").map(Some);
        }
        if matches!(method.as_str(), "Cancel" | "Release") {
            if let Some(answer) = state.answer.take() {
                if let Err(value) = answer.send(JsonValue::parse("false")?) {
                    drop(value);
                }
            }
            state.prompt = None;
            state.answer_owner = None;
            return Ok(Some(message.method_return()));
        }
        let path = message
            .iter_init()
            .get_refarg()
            .and_then(|value| value.as_str().map(str::to_owned));
        if state.target.is_none() || path != state.target {
            return wire::error(&message, "org.bluez.Error.Rejected").map(Some);
        }
    }
    let kind = match method.as_str() {
        "DisplayPinCode" => PromptKind::DisplayPinCode,
        "DisplayPasskey" => PromptKind::DisplayPasskey,
        "RequestConfirmation" => PromptKind::RequestConfirmation,
        "RequestAuthorization" => PromptKind::RequestAuthorization,
        "AuthorizeService" => PromptKind::AuthorizeService,
        "RequestPinCode" => PromptKind::RequestPinCode,
        "RequestPasskey" => PromptKind::RequestPasskey,
        _ => return wire::error(&message, "org.bluez.Error.Rejected").map(Some),
    };
    let value = match prompt_value(&message, kind) {
        Ok(value) => value,
        Err(_) => return wire::error(&message, "org.bluez.Error.Canceled").map(Some),
    };
    let (answer, owner) = {
        let mut state = shared.lock()?;
        if matches!(
            kind,
            PromptKind::DisplayPinCode | PromptKind::DisplayPasskey
        ) {
            state.prompt = Some(Prompt::new(kind, value));
            return Ok(Some(message.method_return()));
        }
        if state
            .answer
            .as_ref()
            .is_some_and(|answer| !answer.is_closed())
        {
            return wire::error(&message, "org.bluez.Error.Canceled").map(Some);
        }
        let (sender, receiver) = oneshot::channel();
        let owner = uuid::Uuid::new_v4();
        state.answer = Some(sender);
        state.answer_owner = Some(owner);
        state.prompt = Some(Prompt::new(kind, value));
        (receiver, owner)
    };
    let received = tokio::select! {
        biased;
        _ = stopping.wait_for(|stop| *stop) => None,
        answer = tokio::time::timeout(Duration::from_secs(60), answer) => answer.ok().and_then(Result::ok),
    };
    shared.finish_answer(owner)?;
    let Some(value) = received else {
        return wire::error(&message, "org.bluez.Error.Canceled").map(Some);
    };
    if matches!(value.view(), JsonView::Bool(false)) {
        return wire::error(&message, "org.bluez.Error.Rejected").map(Some);
    }
    match kind {
        PromptKind::RequestPinCode => {
            let Some(text) = value.to_utf8().filter(|text| !text.contains('\0')) else {
                return Ok(None);
            };
            Ok(Some(message.method_return().append1(text)))
        }
        PromptKind::RequestPasskey => match integer::convert(&value) {
            Unsigned::Value(value) => Ok(Some(message.method_return().append1(value))),
            Unsigned::Invalid => wire::error(&message, "org.bluez.Error.Canceled").map(Some),
            Unsigned::OutOfRange => Ok(None),
        },
        PromptKind::RequestConfirmation
        | PromptKind::RequestAuthorization
        | PromptKind::AuthorizeService
        | PromptKind::DisplayPinCode
        | PromptKind::DisplayPasskey => Ok(Some(message.method_return())),
    }
}

fn prompt_value(message: &Message, kind: PromptKind) -> Result<String, Error> {
    let mut args = message.iter_init();
    args.next();
    match kind {
        PromptKind::DisplayPinCode => Ok(args.read::<String>()?),
        PromptKind::DisplayPasskey | PromptKind::RequestConfirmation => {
            Ok(format!("{:06}", args.read::<u32>()?))
        }
        PromptKind::RequestAuthorization
        | PromptKind::AuthorizeService
        | PromptKind::RequestPinCode
        | PromptKind::RequestPasskey => Ok(String::new()),
    }
}

#[cfg(test)]
mod prompt_owner_tests {
    use super::*;
    use std::{future::poll_fn, future::Future, pin::Pin, task::Poll};

    fn message(method: &str, serial: u32) -> Message {
        let mut message =
            Message::new_method_call("org.carrot.Bluetooth", AGENT, "org.bluez.Agent1", method)
                .unwrap()
                .append1(dbus::Path::new("/fixture/device").unwrap());
        message.set_serial(serial);
        message.set_sender(Some(dbus::strings::BusName::new(":1.42").unwrap()));
        message
    }

    fn shared() -> Shared {
        let shared = Shared::default();
        {
            let mut state = shared.lock().unwrap();
            state.owner = Some(":1.42".into());
            state.target = Some("/fixture/device".into());
        }
        shared
    }

    async fn pending(future: Pin<&mut impl Future>) {
        let mut future = future;
        poll_fn(|context| {
            assert!(future.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn previous_reply_cannot_clear_a_newer_interactive_prompt() {
        for completion in ["respond", "Cancel", "Release"] {
            let shared = shared();
            let (_stop, stopping) = watch::channel(false);
            let mut first = Box::pin(reply(
                message("RequestAuthorization", 1),
                shared.clone(),
                stopping.clone(),
            ));
            pending(first.as_mut()).await;
            let old_id = shared.lock().unwrap().prompt.as_ref().unwrap().id.clone();
            if completion == "respond" {
                shared
                    .respond(&old_id, JsonValue::parse("true").unwrap())
                    .unwrap();
            } else {
                reply(message(completion, 2), shared.clone(), stopping.clone())
                    .await
                    .unwrap()
                    .unwrap();
            }
            // Poll the new real handler before allowing the answered handler to
            // resume. No sleep or D-Bus timing assumption establishes this race.
            let mut second = Box::pin(reply(
                message("RequestAuthorization", 3),
                shared.clone(),
                stopping,
            ));
            pending(second.as_mut()).await;
            let new_id = shared.lock().unwrap().prompt.as_ref().unwrap().id.clone();
            assert_ne!(old_id, new_id);
            first.await.unwrap().unwrap();
            let remaining = shared.lock().unwrap().prompt.as_ref().map(|p| p.id.clone());
            assert_eq!(
                remaining.as_deref(),
                Some(new_id.as_str()),
                "{completion}: old reply erased live new prompt"
            );
            assert!(shared
                .respond(&old_id, JsonValue::parse("true").unwrap())
                .is_err());
            shared
                .respond(&new_id, JsonValue::parse("true").unwrap())
                .unwrap();
            second.await.unwrap().unwrap();
            assert!(shared.lock().unwrap().prompt.is_none());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn display_overlap_still_belongs_to_the_existing_answer() {
        let shared = shared();
        let (_stop, stopping) = watch::channel(false);
        let mut request = Box::pin(reply(
            message("RequestPasskey", 1),
            shared.clone(),
            stopping.clone(),
        ));
        pending(request.as_mut()).await;
        let display = message("DisplayPinCode", 2).append1("display overrides prompt");
        reply(display, shared.clone(), stopping)
            .await
            .unwrap()
            .unwrap();
        let display_id = shared.lock().unwrap().prompt.as_ref().unwrap().id.clone();
        shared
            .respond(&display_id, JsonValue::parse("42").unwrap())
            .unwrap();
        let response = request.await.unwrap().unwrap();
        assert_eq!(response.read1::<u32>().unwrap(), 42);
        assert!(shared.lock().unwrap().prompt.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wrong_sender_or_target_cannot_install_a_prompt() {
        for wrong_sender in [false, true] {
            let shared = shared();
            let (_stop, stopping) = watch::channel(false);
            let mut request = if wrong_sender {
                message("RequestAuthorization", 1)
            } else {
                let mut value = Message::new_method_call(
                    "org.carrot.Bluetooth",
                    AGENT,
                    "org.bluez.Agent1",
                    "RequestAuthorization",
                )
                .unwrap()
                .append1(dbus::Path::new("/wrong").unwrap());
                value.set_serial(1);
                value.set_sender(Some(dbus::strings::BusName::new(":1.42").unwrap()));
                value
            };
            if wrong_sender {
                request.set_sender(Some(dbus::strings::BusName::new(":1.99").unwrap()));
            }
            let mut response = reply(request, shared.clone(), stopping)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                response.as_result().unwrap_err().name(),
                Some("org.bluez.Error.Rejected")
            );
            assert!(shared.lock().unwrap().prompt.is_none());
            assert!(shared.lock().unwrap().answer.is_none());
        }
    }
}
