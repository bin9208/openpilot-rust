//! Serialized S3 frame/command FIFO from cluster_wifi_display.py (final source).
use crate::{Connection, Error, Socket};
use std::collections::VecDeque;

pub trait Channel {
    fn send(&mut self, payload: &[u8], expect_reply: bool) -> Result<u64, Error>;
    fn result(&mut self) -> Result<(u64, Vec<u8>), Error>;
    fn close(&mut self);
}
impl<S: Socket> Channel for Connection<S> {
    fn send(&mut self, payload: &[u8], expect_reply: bool) -> Result<u64, Error> {
        Connection::send(self, payload, expect_reply)
    }
    fn result(&mut self) -> Result<(u64, Vec<u8>), Error> {
        Connection::result(self).map(|(sequence, response, _)| (u64::from(sequence), response))
    }
    fn close(&mut self) {
        let _ = Connection::close(self);
    }
}
#[derive(Debug, thiserror::Error)]
#[error("S3 display disconnected; reconnect required")]
pub struct TransactionError {
    #[source]
    pub cause: Error,
}
pub struct Queue<C: Channel> {
    connection: Option<C>,
    pending: VecDeque<u64>,
    disconnected: bool,
}
impl<C: Channel> Queue<C> {
    pub fn new(connection: Option<C>) -> Self {
        Self {
            connection,
            pending: VecDeque::new(),
            disconnected: false,
        }
    }
    pub fn pending(&self) -> &VecDeque<u64> {
        &self.pending
    }
    pub fn connected(&self) -> bool {
        self.connection.is_some()
    }
    pub fn disconnected(&self) -> bool {
        self.disconnected
    }
    pub fn close(&mut self) {
        if let Some(mut connection) = self.connection.take() {
            connection.close();
        }
        self.pending.clear();
    }
    fn result(&mut self) -> Result<Vec<u8>, Error> {
        let expected = self
            .pending
            .pop_front()
            .expect("pending S3 acknowledgement");
        let (sequence, response) = self
            .connection
            .as_mut()
            .ok_or(Error::DisplayClosed)?
            .result()?;
        if sequence != expected {
            return Err(Error::AckSequence);
        }
        Ok(response)
    }
    /// Exclusive access serializes the complete drain/send/receive transaction.
    pub fn transaction(
        &mut self,
        payload: &[u8],
        frame: bool,
        expect_reply: bool,
        mut connected: impl FnMut(bool),
    ) -> Result<Vec<u8>, TransactionError> {
        let result = (|| {
            if self.connection.is_none() {
                return Err(Error::DisplayClosed);
            }
            while if frame {
                self.pending.len() >= 3
            } else {
                !self.pending.is_empty()
            } {
                self.result()?;
            }
            let sequence = self
                .connection
                .as_mut()
                .expect("checked S3 connection")
                .send(payload, expect_reply)?;
            self.pending.push_back(sequence);
            if frame {
                Ok(Vec::new())
            } else {
                self.result()
            }
        })();
        result.map_err(|cause| {
            self.disconnected = true;
            connected(false);
            if let Some(mut connection) = self.connection.take() {
                connection.close();
            }
            TransactionError { cause }
        })
    }
}
