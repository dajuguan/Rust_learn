//! Case study: keeping a future alive across `select!` iterations.
//!
//! Distilled from the Base mainnet outage postmortem (June 25, 2025) and the
//! follow-up fix in base/base#3805. The sequencer's initial engine reset was
//! written as an inline branch of a `select!` loop that also serviced admin
//! queries. When an admin query won the `select!`, the *freshly created* reset
//! future was dropped together with its in-flight response receiver, so the
//! reset silently never completed and the sequencer could not catch up to tip.
//!
//! The lesson is a generic Rust async idiom, not blockchain-specific:
//!
//!   In `select!`, when one branch completes the others' futures are DROPPED.
//!   A future that is *constructed inline in a branch* is therefore re-created
//!   (and cancelled) every iteration. If that future holds in-flight state
//!   (a sent request, a oneshot receiver, a timer), the state is lost.
//!
//! Fix: move the future's ownership OUT of the `select!` so only a `&mut`
//! borrow is polled there. Dropping a `&mut F` does nothing to `F`, so the
//! future survives across iterations. Two ways to hold it:
//!
//!   1. `Option<BoxFuture<..>>`  -- heap-allocated, type-erased (Base's fix).
//!   2. `Fuse::terminated()` + `tokio::pin!` -- stack-pinned, no allocation.
//!
//! Both are correct. (2) is tidier: the "empty vs in-flight" slot is modeled
//! by the type, there is no `as_mut().expect(...)` panic path, and the future
//! is overwritten in place via `Pin::set` instead of re-`Box::pin`-ed.


/*
cd /home/po/self/Rust_learn/concepts
cargo test -p future_async select_cancellation
需要comment掉test_self_referential_mypin_correct_shorter_lifetime(故意留的编译不过的bug)
*/

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures::future::{BoxFuture, Fuse, FusedFuture, FutureExt};
use tokio::sync::mpsc;
use tokio::time::sleep;

/// An admin query arrives faster than the engine answers a reset.
const ADMIN_INTERVAL: Duration = Duration::from_millis(20);
/// How long the (simulated) engine takes to answer a reset request.
const RESPONSE_LATENCY: Duration = Duration::from_millis(60);

#[derive(Debug)]
struct ResetError;

/// Simulates `engine_client.reset_engine_forkchoice()`.
///
/// The first poll "dispatches a request and allocates a response receiver"
/// (we bump `requests_sent`); the response only arrives after `RESPONSE_LATENCY`.
/// If this future is dropped before the sleep resolves, the request/receiver is
/// lost -- exactly the receiver that the real bug dropped.
async fn engine_reset(requests_sent: Arc<AtomicUsize>) -> Result<(), ResetError> {
    requests_sent.fetch_add(1, Ordering::SeqCst);
    sleep(RESPONSE_LATENCY).await; // awaiting the engine's response
    Ok(())
}

/// Spawns a producer that emits `n` admin queries, then closes the channel.
/// Once closed, `recv()` yields `None`, so the admin branch goes inert and the
/// reset is finally allowed to finish -- letting the test terminate.
fn spawn_admin(n: usize) -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for _ in 0..n {
            sleep(ADMIN_INTERVAL).await;
            if tx.send(()).await.is_err() {
                break;
            }
        }
        // tx dropped here -> channel closes.
    });
    rx
}

/// BUGGY: the reset future is built inline in the `select!` branch, so every
/// admin query that wins the race drops and discards it. Returns how many reset
/// requests were dispatched -- one is real, the rest are wasted/lost receivers.
pub async fn run_buggy(mut admin_rx: mpsc::Receiver<()>, requests_sent: Arc<AtomicUsize>) -> usize {
    let mut admin_handled = 0usize;
    loop {
        tokio::select! {
            // Pattern `Some(_)` fails once the channel closes, disabling the
            // branch and letting the reset branch run to completion.
            Some(_q) = admin_rx.recv() => { admin_handled += 1; }
            // A NEW future is created on every iteration; losing the previous
            // one (and its in-flight response) is the bug.
            res = engine_reset(Arc::clone(&requests_sent)) => {
                let _ = res;
                return admin_handled;
            }
        }
    }
}

/// FIX 1 (Base's): hold the future in `Option<BoxFuture>`; `select!` only polls
/// a `&mut` to it, so an admin query no longer drops it. Re-armed once per retry
/// (here: once), each re-arm heap-allocates a fresh box.
pub async fn run_option_boxed(
    mut admin_rx: mpsc::Receiver<()>,
    requests_sent: Arc<AtomicUsize>,
) -> usize {
    let mut resetting: Option<BoxFuture<'static, Result<(), ResetError>>> = None;
    let mut admin_handled = 0usize;
    loop {
        if resetting.is_none() {
            resetting = Some(engine_reset(Arc::clone(&requests_sent)).boxed());
        }
        // `expect` is unreachable but is a panic path a reader must verify away.
        let reset = resetting.as_mut().expect("reset future armed before polling");
        tokio::select! {
            Some(_q) = admin_rx.recv() => { admin_handled += 1; }
            res = reset => {
                resetting = None; // ELSyncing would loop and re-arm here.
                let _ = res;
                return admin_handled;
            }
        }
    }
}

/// FIX 2 (idiomatic): a stack-pinned `Fuse::terminated()` slot. `is_terminated()`
/// models "empty"; `Pin::set` overwrites the future IN PLACE (same stack slot,
/// no allocation) because the async block is one concrete type of fixed size.
pub async fn run_fuse(mut admin_rx: mpsc::Receiver<()>, requests_sent: Arc<AtomicUsize>) -> usize {
    let resetting = Fuse::terminated();
    tokio::pin!(resetting);
    let mut admin_handled = 0usize;
    loop {
        if resetting.is_terminated() {
            resetting.set(engine_reset(Arc::clone(&requests_sent)).fuse());
        }
        tokio::select! {
            Some(_q) = admin_rx.recv() => { admin_handled += 1; }
            // Dropping this `&mut` borrow leaves the pinned future untouched.
            res = &mut resetting => {
                let _ = res; // ELSyncing would loop; the fuse is now terminated,
                             // so the next iteration re-arms the SAME slot.
                return admin_handled;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inline future is recreated on every admin query, so far more reset
    /// requests are dispatched than the single one that should exist.
    #[tokio::test]
    async fn buggy_loses_inflight_resets() {
        let requests_sent = Arc::new(AtomicUsize::new(0));
        let admin_rx = spawn_admin(3);
        let handled = run_buggy(admin_rx, Arc::clone(&requests_sent)).await;

        let sent = requests_sent.load(Ordering::SeqCst);
        println!("buggy: requests_sent={sent}, admin_handled={handled}");
        assert!(handled >= 1, "admin queries should still be serviced");
        assert!(
            sent > 1,
            "each interrupted reset is dropped and re-dispatched; got {sent}"
        );
    }

    /// Hoisting into `Option<BoxFuture>` keeps the reset alive: exactly one
    /// request is dispatched even though admin queries are serviced meanwhile.
    #[tokio::test]
    async fn option_boxed_preserves_reset() {
        let requests_sent = Arc::new(AtomicUsize::new(0));
        let admin_rx = spawn_admin(3);
        let handled = run_option_boxed(admin_rx, Arc::clone(&requests_sent)).await;

        let sent = requests_sent.load(Ordering::SeqCst);
        println!("option_boxed: requests_sent={sent}, admin_handled={handled}");
        assert_eq!(sent, 1, "the in-flight reset must survive admin queries");
    }

    /// `Fuse` slot behaves identically to the box version, without the heap
    /// allocation or the `unwrap` panic path.
    #[tokio::test]
    async fn fuse_preserves_reset() {
        let requests_sent = Arc::new(AtomicUsize::new(0));
        let admin_rx = spawn_admin(3);
        let handled = run_fuse(admin_rx, Arc::clone(&requests_sent)).await;

        let sent = requests_sent.load(Ordering::SeqCst);
        println!("fuse: requests_sent={sent}, admin_handled={handled}");
        assert_eq!(sent, 1, "the in-flight reset must survive admin queries");
    }
}
