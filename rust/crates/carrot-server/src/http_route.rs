use crate::{
    http::{decode_request, Application, Body},
    http_response::{error_response, text},
    http_routes::dispatch,
    Error,
};
use hyper::{body::Incoming, header, Request, Response, StatusCode};
use std::{convert::Infallible, sync::Arc};

pub(crate) async fn route(
    mut request: Request<Incoming>,
    app: Arc<Application>,
    sound: crate::web_sound_http::Sessions,
) -> Result<Response<Body>, Infallible> {
    let context = crate::request_body::DecodeContext::default();
    request.extensions_mut().insert(context.clone());
    let head = request.method() == hyper::Method::HEAD;
    let mut request = decode_request(request);
    let prefetched =
        std::future::poll_fn(|cx| std::task::Poll::Ready(request.body_mut().prefetch(cx))).await;
    let mut response = match prefetched {
        Ok(()) => {
            if let Some(expectation) = request
                .extensions()
                .get::<hyper::ext::RawConditionalHeaders>()
                .and_then(|headers| headers.get(&header::EXPECT))
                .or_else(|| request.headers().get(header::EXPECT))
                .filter(|value| {
                    request.version() == hyper::Version::HTTP_11 && !value.as_bytes().is_empty()
                })
            {
                if expectation.as_bytes().eq_ignore_ascii_case(b"100-continue") {
                    match request
                        .extensions()
                        .get::<hyper::ext::ContinueSignal>()
                        .map(hyper::ext::ContinueSignal::send)
                        .transpose()
                    {
                        Ok(_) => dispatch(request, app, sound).await?,
                        Err(error) => error_response(error.to_string(), head),
                    }
                } else {
                    let expectation = String::from_utf8_lossy(expectation.as_bytes());
                    text(
                        StatusCode::EXPECTATION_FAILED,
                        &format!("Unknown Expect: {expectation}"),
                        head,
                    )
                }
            } else {
                dispatch(request, app, sound).await?
            }
        }
        Err(failure) => {
            let error = Error::Request(failure);
            crate::http_response::parser_response(&error, head)
                .unwrap_or_else(|| error_response(error.to_string(), head))
        }
    };
    if context.requires_close() {
        response
            .extensions_mut()
            .insert(hyper::ext::CloseAfterResponse);
    }
    Ok(response)
}
