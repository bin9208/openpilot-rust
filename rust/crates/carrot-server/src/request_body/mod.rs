//! Lazy HTTP content decoding shared by JSON read and multipart field consumers.
mod decoder;
use bytes::Bytes;
use decoder::Decoder;
use hyper::{
    body::{Body, Frame, Incoming, SizeHint},
    HeaderMap, Request,
};
use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

#[derive(Clone, Debug, thiserror::Error)]
pub enum DecodeFailure {
    #[error("{message}")]
    Payload { message: String },
    #[error("{message}")]
    Parser { message: String },
}

impl DecodeFailure {
    pub const fn is_parser(&self) -> bool {
        matches!(self, Self::Parser { .. })
    }
    pub const fn requires_close(&self) -> bool {
        true
    }
    pub fn with_message(self, message: String) -> Self {
        match self {
            Self::Payload { .. } => Self::Payload { message },
            Self::Parser { .. } => Self::Parser { message },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DecodeContext(Arc<AtomicBool>);

impl DecodeContext {
    pub(crate) fn requires_close(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub(crate) fn record_failure(&self) {
        self.0.store(true, Ordering::Release);
    }
}

pub struct DecodedBody {
    incoming: Incoming,
    decoder: Decoder,
    context: DecodeContext,
    prefetched: VecDeque<Frame<Bytes>>,
    failure: Option<DecodeFailure>,
    finished: bool,
}

pub fn decode_request(mut request: Request<Incoming>) -> Request<DecodedBody> {
    let context = request
        .extensions()
        .get::<DecodeContext>()
        .cloned()
        .unwrap_or_default();
    request.extensions_mut().insert(context.clone());
    let (parts, body) = request.into_parts();
    let body = decoded(&parts.headers, body).with_context(context);
    Request::from_parts(parts, body)
}

pub(crate) fn decoded(headers: &HeaderMap, incoming: Incoming) -> DecodedBody {
    DecodedBody {
        incoming,
        decoder: Decoder::new(headers),
        context: DecodeContext::default(),
        prefetched: VecDeque::new(),
        failure: None,
        finished: false,
    }
}

impl DecodedBody {
    pub(crate) fn with_context(mut self, context: DecodeContext) -> Self {
        self.context = context;
        self
    }

    /// Inspect already-framed data without requesting another read or waiting.
    ///
    /// # Errors
    /// Returns an observed parser-phase EOF failure; payload failures remain for
    /// the feature's first read and retain connection-close intent.
    pub(crate) fn prefetch(&mut self, cx: &mut Context<'_>) -> Result<(), DecodeFailure> {
        loop {
            match self.poll_decoded_frame(cx, true) {
                Poll::Pending | Poll::Ready(None) => return Ok(()),
                Poll::Ready(Some(Ok(frame))) => self.prefetched.push_back(frame),
                Poll::Ready(Some(Err(failure))) => {
                    let parser = failure.is_parser();
                    self.failure = Some(failure.clone());
                    return if parser { Err(failure) } else { Ok(()) };
                }
            }
        }
    }

    fn fail(
        &mut self,
        failure: DecodeFailure,
    ) -> Poll<Option<Result<Frame<Bytes>, DecodeFailure>>> {
        self.context.record_failure();
        self.finished = true;
        Poll::Ready(Some(Err(failure)))
    }

    fn poll_decoded_frame(
        &mut self,
        cx: &mut Context<'_>,
        buffered_only: bool,
    ) -> Poll<Option<Result<Frame<Bytes>, DecodeFailure>>> {
        if self.finished {
            return Poll::Ready(None);
        }
        loop {
            let incoming = if buffered_only {
                Pin::new(&mut self.incoming).poll_buffered_frame(cx)
            } else {
                Pin::new(&mut self.incoming).poll_frame(cx)
            };
            match incoming {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => match self.decoder.finish() {
                    Ok(()) => {
                        self.finished = true;
                        return Poll::Ready(None);
                    }
                    Err(failure) => return self.fail(failure),
                },
                Poll::Ready(Some(Err(failure))) => {
                    return self.fail(DecodeFailure::Payload {
                        message: failure.to_string(),
                    });
                }
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Err(frame) => return Poll::Ready(Some(Ok(frame))),
                    Ok(bytes) if self.decoder.is_identity() => {
                        return Poll::Ready(Some(Ok(Frame::data(bytes))));
                    }
                    Ok(bytes) => match self.decoder.feed(&bytes) {
                        Err(failure) => return self.fail(failure),
                        Ok(bytes) if !bytes.is_empty() => {
                            return Poll::Ready(Some(Ok(Frame::data(Bytes::from(bytes)))));
                        }
                        Ok(_) => {}
                    },
                },
            }
        }
    }
}

impl Body for DecodedBody {
    type Data = Bytes;
    type Error = DecodeFailure;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, DecodeFailure>>> {
        let this = self.get_mut();
        if let Some(failure) = this.failure.take() {
            this.prefetched.clear();
            return Poll::Ready(Some(Err(failure)));
        }
        if let Some(frame) = this.prefetched.pop_front() {
            return Poll::Ready(Some(Ok(frame)));
        }
        this.poll_decoded_frame(cx, false)
    }

    fn is_end_stream(&self) -> bool {
        self.finished && self.prefetched.is_empty() && self.failure.is_none()
    }
    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}
