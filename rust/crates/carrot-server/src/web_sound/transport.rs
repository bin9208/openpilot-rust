use super::Shutdown;
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::watch,
};

pub(crate) struct Transport {
    io: TokioIo<Upgraded>,
    shutdown: watch::Receiver<Shutdown>,
}

impl Transport {
    pub fn new(upgraded: Upgraded, shutdown: watch::Receiver<Shutdown>) -> Self {
        Self {
            io: TokioIo::new(upgraded),
            shutdown,
        }
    }
}

impl AsyncRead for Transport {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if *this.shutdown.borrow() == Shutdown::Running {
            return Pin::new(&mut this.io).poll_read(context, output);
        }
        for _ in 0..8 {
            let mut discarded = [0_u8; 8192];
            let mut input = ReadBuf::new(&mut discarded);
            match Pin::new(&mut this.io).poll_read(context, &mut input) {
                Poll::Ready(Ok(())) if input.filled().is_empty() => return Poll::Ready(Ok(())),
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
        context.waker().wake_by_ref();
        Poll::Pending
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write(context, bytes)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().io).poll_shutdown(context)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().io).poll_write_vectored(context, bytes)
    }
}
