use std::{
    future::{poll_fn, Future},
    task::Poll,
    thread,
};
use tokio::sync::mpsc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixed = std::env::args().nth(1).as_deref() == Some("fixed");
    let (sender, mut receiver) = mpsc::channel(16);
    for value in 0_u8..16 {
        sender.try_send(value)?;
    }
    let sending = sender.send(17);
    tokio::pin!(sending);
    let pending = poll_fn(|context| Poll::Ready(sending.as_mut().poll(context).is_pending())).await;
    if !pending {
        return Err("17th bounded send was not pending".into());
    }
    println!("{{\"send_pending\":true}}");
    let owner = thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(async {
            if receiver.recv().await != Some(0) {
                return Err("queue order changed".into());
            }
            receiver.close();
            if fixed {
                let mut drained = 0;
                while receiver.try_recv().is_ok() {
                    drained += 1;
                }
                drop(receiver);
                println!("{{\"receiver_dropped\":true,\"drained\":{drained}}}");
            } else {
                let drain = async { while receiver.recv().await.is_some() {} };
                tokio::pin!(drain);
                let blocked =
                    poll_fn(|context| Poll::Ready(drain.as_mut().poll(context).is_pending())).await;
                println!("{{\"closed_drain_pending\":{blocked}}}");
                drain.await;
            }
            Ok(())
        })
    });
    owner
        .join()
        .map_err(|_| "owned queue thread panicked")?
        .map_err(std::io::Error::other)?;
    println!("{{\"owner_joined\":true}}");
    Ok(())
}
