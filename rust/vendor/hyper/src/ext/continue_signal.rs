use crate::common::watch;
use std::{
    io,
    sync::{Arc, Mutex},
    task::Context,
};

#[derive(Clone)]
/// Application-controlled HTTP/1 Continue; the dispatcher retains its sender until consumption.
pub struct ContinueSignal {
    sender: Arc<Mutex<watch::Sender>>,
}

impl ContinueSignal {
    /// Request one informational response; repeating this call cannot queue another response.
    pub fn send(&self) -> io::Result<()> {
        self.sender
            .lock()
            .map_err(|_| io::Error::other("Continue signal lock poisoned"))?
            .send(2);
        Ok(())
    }

    pub(crate) fn channel() -> (Self, Receiver) {
        let (sender, receiver) = watch::channel(1);
        let signal = Self {
            sender: Arc::new(Mutex::new(sender)),
        };
        let receiver = Receiver {
            _sender_guard: signal.clone(),
            receiver,
        };
        (signal, receiver)
    }
}

impl std::fmt::Debug for ContinueSignal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContinueSignal")
            .finish_non_exhaustive()
    }
}

pub(crate) struct Receiver {
    _sender_guard: ContinueSignal,
    receiver: watch::Receiver,
}

impl Receiver {
    pub(crate) fn requested(&mut self, context: &mut Context<'_>) -> bool {
        self.receiver.load(context) == 2
    }
}
