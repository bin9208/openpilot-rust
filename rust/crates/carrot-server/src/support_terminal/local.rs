use super::{context::Context, model::Session};
use crate::Error;
use hyper_util::rt::TokioIo;
use std::{convert::Infallible, rc::Rc};
use tokio::{net::TcpListener, sync::watch, task::JoinSet};

pub(super) async fn run(
    context: Rc<Context>,
    listener: TcpListener,
    session: Session,
    mut stop: watch::Receiver<bool>,
) -> Result<(), Error> {
    let _session = session;
    let mut connections = JoinSet::new();
    loop {
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            accepted = listener.accept() => {
                let (socket, _) = accepted?;
                let context = Rc::clone(&context);
                connections.spawn_local(async move {
                    hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(socket), hyper::service::service_fn(move |request| {
                        let context = Rc::clone(&context);
                        async move { Ok::<_, Infallible>(super::http::guest(crate::http::decode_request(request), context).await) }
                    })).with_upgrades().await.map_err(|error| Error::Source(error.to_string()))
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {},
        }
    }
    drop(listener);
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}
