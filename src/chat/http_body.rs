//! One-chunk response pump. Its independent task owns the source body and HTTP permits,
//! so expiry frees both even when the receiver never polls its body again.
use super::server::HttpLimits;
use axum::body::{Body, Bytes};
use futures_util::{Stream, StreamExt};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
type Chunk = Result<Bytes, std::io::Error>;
#[derive(Default)]
struct State {
    chunk: Option<Chunk>,
    done: bool,
    waker: Option<Waker>,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    space: Notify,
}
struct Receiver {
    shared: Arc<Shared>,
    cancel: CancellationToken,
}
impl Stream for Receiver {
    type Item = Chunk;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(chunk) = state.chunk.take() {
            self.shared.space.notify_one();
            return Poll::Ready(Some(chunk));
        }
        if state.done {
            return Poll::Ready(None);
        }
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
fn end(shared: &Shared) {
    let mut state = shared.state.lock().unwrap();
    state.chunk = None;
    state.done = true;
    if let Some(waker) = state.waker.take() {
        waker.wake();
    }
}
pub(super) fn bounded(
    body: Body,
    limits: HttpLimits,
    deadline: tokio::time::Instant,
    stop: CancellationToken,
    guard: impl Send + 'static,
) -> Body {
    let shared = Arc::new(Shared::default());
    let cancel = stop.child_token();
    let receiver = Receiver {
        shared: shared.clone(),
        cancel: cancel.clone(),
    };
    tokio::spawn(async move {
        let _guard = guard;
        let mut source = body.into_data_stream();
        let pump = async {
            loop {
                let ready = async {
                    loop {
                        let space = shared.space.notified();
                        tokio::pin!(space);
                        space.as_mut().enable();
                        if shared.state.lock().unwrap().chunk.is_none() {
                            break;
                        }
                        space.await;
                    }
                };
                if tokio::time::timeout(limits.response_idle_timeout, ready)
                    .await
                    .is_err()
                {
                    break;
                }
                // Producer silence is not write backpressure. The outer lifetime timer
                // bounds it without prematurely ending a legitimate quiet long wait.
                let item = match source.next().await {
                    Some(item) => {
                        item.map_err(|_| std::io::Error::other("HTTP response stream failed"))
                    }
                    None => break,
                };
                let mut state = shared.state.lock().unwrap();
                state.chunk = Some(item);
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
            }
        };
        tokio::select! {_=pump=>{},_=tokio::time::sleep_until(deadline)=>{},_=cancel.cancelled()=>{}}
        end(&shared);
        // Dropping source and guard happens here independently of Receiver polling.
    });
    Body::from_stream(receiver)
}
