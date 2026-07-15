//! Two-phase non-blocking "must-deliver" send: `poll_reserve` + `send_item`.
//!
//! Inside a hand-written `Future::poll` you cannot `.await`, yet sometimes you
//! must emit a critical message that can NEITHER be dropped NOR block the poll
//! (e.g. "I am terminating"). Three ways to push into a bounded `mpsc`:
//!
//!   - `send().await`        -> blocks (suspends) until there is room. BACKPRESSURE.
//!                              Correct in async fns, but forbidden in a bare poll.
//!   - `try_send(msg)`       -> if the buffer is full it DROPS the message.
//!                              Fine for load-shedding, fatal for a "must-deliver".
//!   - `poll_reserve(cx)`    -> in poll semantics, RESERVE one capacity slot. Once
//!     + `send_item(msg)`       it returns `Poll::Ready(Ok(()))`, the slot is ours
//!                              and `send_item` is GUARANTEED to succeed. If the
//!                              channel is full it returns `Poll::Pending` (waker
//!                              registered): keep the message, retry next poll. No
//!                              drop, no block — progress is driven by re-polling.
//!
//! This is exactly reth's `ActiveSession::poll_terminate_message`
//! (crates/net/network/src/session/active.rs:608-625): it stashes
//! `(PollSender, msg)`, `poll_reserve`s each poll, re-stashes on `Pending`, and
//! only `send_item`s the termination message once a slot is reserved.
//!
//! Demo shape: a bounded channel of capacity 2 is pre-filled to the brim so the
//! sender's first reserves come back `Pending`. A spawned sender task runs a
//! hand-rolled `poll_fn` holding one critical message and keeps reserving. The
//! consumer (main task) sleeps briefly — letting the sender fail a few reserves
//! — then drains the channel, freeing a slot so the reserve finally succeeds and
//! the critical message is delivered exactly once, without the sender ever
//! blocking.

use std::future::poll_fn;
use std::task::Poll;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::PollSender;

/// Bounded channel capacity. Tiny on purpose: pre-filling CAP items leaves zero
/// room, so the sender's `poll_reserve` starts out `Pending`.
const CAP: usize = 2;

/// Messages on the channel. Tagging pre-fill vs critical lets the test prove the
/// critical one arrives exactly once with the right payload, alongside the
/// pre-filled items (total conservation).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Msg {
    /// One of the CAP items used to jam the buffer full up front.
    Prefill(u64),
    /// The one message that must be delivered — never dropped, never blocked on.
    Critical(&'static str),
}

/// Sender task: owns a `PollSender` and ONE critical message, and drives a
/// hand-written `poll_fn` (no `.await` on the send path). Each poll it
/// `poll_reserve`s:
///   - `Pending`      -> buffer still full; keep the message, bump the counter,
///                       return `Pending` (waker is registered, so freeing a
///                       slot re-polls us). The sender never blocks.
///   - `Ready(Ok)`    -> a slot is reserved; `send_item` is now guaranteed to
///                       succeed. Deliver once and finish.
///   - `Ready(Err)`   -> receiver gone; nothing to do.
///
/// Returns how many times `poll_reserve` came back `Pending` (proof the
/// two-phase path actually exercised backpressure without dropping/blocking).
async fn sender_task(tx: mpsc::Sender<Msg>, critical: Msg) -> usize {
    let mut poll_sender = PollSender::new(tx);
    // The message lives here between polls — the essence of "must-deliver":
    // we hold onto it until a slot is reserved, then hand it over exactly once.
    let mut pending_msg = Some(critical);
    let mut pending_reserves = 0usize;

    poll_fn(|cx| {
        // `poll_sender` needs `&mut`; the `FnMut` closure captures it mutably.
        match poll_sender.poll_reserve(cx) {
            Poll::Pending => {
                pending_reserves += 1;
                println!("[sender] channel full, cannot reserve yet, will retry");
                Poll::Pending
            }
            Poll::Ready(Ok(())) => {
                let msg = pending_msg.take().expect("critical message reserved twice");
                // Guaranteed to succeed: we hold a reserved permit.
                poll_sender
                    .send_item(msg)
                    .expect("send_item after a successful poll_reserve must not fail");
                println!("[sender] reserved a slot, critical message delivered");
                Poll::Ready(pending_reserves)
            }
            Poll::Ready(Err(_)) => {
                // Channel closed; there is nothing we can do with the message.
                println!("[sender] channel closed before reserve succeeded");
                Poll::Ready(pending_reserves)
            }
        }
    })
    .await
}

/// Consumer: sleep first so the (already-full) channel forces the sender through
/// several failed reserves, then drain exactly `expected_total` messages. Each
/// `recv` frees a slot, which wakes the sender and lets its reserve succeed.
async fn consumer(mut rx: mpsc::Receiver<Msg>, expected_total: usize) -> Vec<Msg> {
    // Delay consumption so the sender experiences reserve-`Pending` first.
    tokio::time::sleep(Duration::from_millis(20)).await;

    let mut received = Vec::with_capacity(expected_total);
    while received.len() < expected_total {
        match rx.recv().await {
            Some(msg) => {
                println!("[consumer] received {msg:?}");
                received.push(msg);
            }
            None => break, // all senders dropped
        }
    }
    received
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn critical_message_is_reserved_then_delivered_exactly_once() {
        let (tx, rx) = mpsc::channel::<Msg>(CAP);

        // Jam the buffer full: CAP pre-fill items => the sender's first reserves
        // will be `Pending`.
        for i in 0..CAP as u64 {
            tx.send(Msg::Prefill(i)).await.unwrap();
        }

        let critical = Msg::Critical("SHUTDOWN");
        // Move `tx` into the sender; when its `PollSender` drops, the channel
        // closes. The consumer stops on count, so it never depends on that.
        let sender = tokio::spawn(sender_task(tx, critical.clone()));

        // We expect the CAP pre-filled items plus the one critical message.
        let expected_total = CAP + 1;
        let received = consumer(rx, expected_total).await;
        let pending_reserves = sender.await.unwrap();

        println!(
            "received {} msg(s), sender saw {pending_reserves} reserve-Pending(s): {received:?}",
            received.len()
        );

        // (c) Total conservation: every pre-filled item plus the critical one.
        assert_eq!(received.len(), expected_total, "no message lost");
        for i in 0..CAP as u64 {
            assert!(
                received.contains(&Msg::Prefill(i)),
                "pre-filled item {i} must be received"
            );
        }

        // (a) + (b) The critical message arrived, with the right payload, and
        // EXACTLY once (reserved once, sent once — never dropped, never duped).
        let critical_hits = received.iter().filter(|m| **m == critical).count();
        assert_eq!(
            critical_hits, 1,
            "critical message must be delivered exactly once"
        );
        assert_eq!(
            received.last(),
            Some(&critical),
            "critical lands in the slot freed after the pre-fill drains"
        );

        // (d) The sender actually hit backpressure: it returned `Pending` at
        // least once (full channel) yet still delivered — non-blocking must-send.
        assert!(
            pending_reserves >= 1,
            "sender should have reserved-Pending at least once against the full channel"
        );
    }
}
