//! Waiting on other threads without holding the background pool (review,
//! D-066). GPUI runs background tasks on the Windows thread pool, whose work
//! items should be short (Microsoft Learn, "Best practices for using the
//! thread pool"); a task blocked in `recv()` holds a pool thread for as long
//! as it waits, and a dozen long waits (watchers, services, the listing)
//! starve everything else. Here the blocking wait happens on a thread of its
//! own, and the UI side awaits a futures channel.

use std::sync::mpsc::Receiver;

use futures::channel::{mpsc, oneshot};

/// Every message of `rx`, as a stream to await. Ends when the sender is
/// gone; the waiting thread ends when nobody listens any more.
pub fn stream<T: Send + 'static>(rx: Receiver<T>) -> mpsc::UnboundedReceiver<T> {
    let (tx, out) = mpsc::unbounded();
    // If the thread cannot start, `tx` goes with the closure and the stream
    // just ends.
    let _ = std::thread::Builder::new()
        .name("fx-wait".into())
        .spawn(move || {
            for item in rx {
                if tx.unbounded_send(item).is_err() {
                    break;
                }
            }
        });
    out
}

/// The next message of `rx`; `None` when its sender is gone.
pub async fn next<T: Send + 'static>(rx: Receiver<T>) -> Option<T> {
    let (tx, out) = oneshot::channel();
    let _ = std::thread::Builder::new()
        .name("fx-wait".into())
        .spawn(move || {
            let _ = tx.send(rx.recv().ok());
        });
    out.await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    #[test]
    fn streams_and_single_messages_arrive_in_order() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut s = super::stream(rx);
        for k in 0..3 {
            tx.send(k).unwrap();
        }
        drop(tx);
        let got: Vec<i32> = futures::executor::block_on(async {
            let mut v = Vec::new();
            while let Some(k) = s.next().await {
                v.push(k);
            }
            v
        });
        assert_eq!(got, [0, 1, 2]);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send("um").unwrap();
        assert_eq!(futures::executor::block_on(super::next(rx)), Some("um"));
        let (tx, rx) = std::sync::mpsc::channel::<u8>();
        drop(tx);
        assert_eq!(futures::executor::block_on(super::next(rx)), None);
    }
}
