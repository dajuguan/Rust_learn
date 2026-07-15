//! Minimal experiment: verify tokio's cooperative scheduling budget.
//!
//! References:
//!   - https://tokio.rs/blog/2020-04-preemption
//!   - https://github.com/tokio-rs/tokio/pull/2160
//!   - https://github.com/rust-lang/futures-rs/issues/1957
//!
//! Idea: each task is given a "budget" (default 128) every time it is polled.
//! Each successful poll of a coop-aware resource (here a channel's poll_recv,
//! one Ready = one unit) decrements the budget. Once the budget is exhausted,
//! the resource returns Pending to yield even though data is still ready, and
//! wakes the task itself, so the next round the budget is refilled to 128.
//!
//! Observation: pre-fill an unbounded channel (data is always ready, so it can
//! never go Pending for "no data"), then hand-poll poll_recv in a loop and count
//! how many recvs succeed before we hit Pending. Expected: exactly 128 per round.

use std::future::poll_fn;
use std::task::Poll;

use tokio::sync::mpsc;

/// tokio's cooperative scheduling budget per task poll.
#[cfg(test)]
const COOP_BUDGET: usize = 128;

/// Result of one `observe_yields` run.
#[cfg(test)]
struct Observation {
    /// How many recvs succeeded in each round before a coop yield (or drain end).
    rounds: Vec<usize>,
    /// How many times the neighbor task got to run (once per coop yield window).
    neighbor_ticks: usize,
}

/// Drain `total` pre-filled items from an unbounded channel by hand-polling
/// `poll_recv`, while a neighbor task also wants to run. On a single-threaded
/// runtime the neighbor can only make progress when the draining task yields —
/// which happens exactly at each coop-budget boundary. So its prints interleave
/// one per coop yield, demonstrating what the budget buys us.
#[cfg(test)]
fn observe_yields(total: u64) -> Observation {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        // Neighbor task: prints a tick, then cooperatively yields and waits to be
        // scheduled again. It is merely *queued* here; it runs only in the gaps
        // the draining task leaves behind at each coop yield.
        let ticks = Arc::new(AtomicUsize::new(0));
        let neighbor_ticks = ticks.clone();
        tokio::spawn(async move {
            loop {
                let k = neighbor_ticks.fetch_add(1, Ordering::SeqCst);
                println!("  [neighbor] got a turn to run (tick {k})");
                tokio::task::yield_now().await;
            }
        });

        let (tx, mut rx) = mpsc::unbounded_channel::<u64>();

        // Pre-fill with data so poll_recv is always "data ready"; this makes the
        // coop budget the only possible cause of a Pending.
        for i in 0..total {
            tx.send(i).unwrap();
        }
        drop(tx); // close the sender: poll_recv returns Ready(None) once drained

        let mut rounds = Vec::new();
        let mut count_in_round = 0usize;

        poll_fn(|cx| {
            loop {
                match rx.poll_recv(cx) {
                    Poll::Ready(Some(_)) => count_in_round += 1,
                    Poll::Ready(None) => {
                        // channel closed and fully drained; record the tail round
                        rounds.push(count_in_round);
                        return Poll::Ready(());
                    }
                    Poll::Pending => {
                        // Data is still there; this Pending is the coop budget
                        // being exhausted and forcing a yield. The neighbor task
                        // gets to run in the window this yield opens up.
                        println!("coop yield after {count_in_round} recvs in this round");
                        rounds.push(count_in_round);
                        count_in_round = 0;
                        // coop already called cx.waker().wake() on yield, so the
                        // runtime re-polls this task after servicing the queue
                        // (i.e. after the neighbor runs) with a fresh budget.
                        return Poll::Pending;
                    }
                }
            }
        })
        .await;

        Observation {
            rounds,
            neighbor_ticks: ticks.load(Ordering::SeqCst),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yields_every_128_polls() {
        let tail = 10;
        let full_rounds = 3;
        let total = COOP_BUDGET as u64 * full_rounds + tail;

        let obs = observe_yields(total);
        println!(
            "per-round recv counts: {:?}; neighbor ran {} time(s)",
            obs.rounds, obs.neighbor_ticks
        );

        // Every complete round should consume exactly the full budget (128),
        // and the last round holds the leftover tail.
        assert_eq!(obs.rounds.len() as u64, full_rounds + 1);
        for &c in &obs.rounds[..full_rounds as usize] {
            assert_eq!(
                c, COOP_BUDGET,
                "each full round must be exactly the coop budget"
            );
        }
        assert_eq!(*obs.rounds.last().unwrap() as u64, tail);

        // The neighbor could only run in the windows opened by coop yields, so
        // it must have run at least once (had the drainer never yielded, the
        // neighbor would have been starved to zero).
        assert!(
            obs.neighbor_ticks >= 1,
            "neighbor should have run in the gaps created by coop yields"
        );
    }

    /// Counter-example: tokio is cooperative, NOT preemptive. Code that runs
    /// without hitting a `.await` cannot be preempted — the coop budget only
    /// helps tasks that *do* await (see `yields_every_128_polls`). Here a plain
    /// synchronous loop on a single-threaded runtime starves a neighbor task
    /// until we hand control back with an explicit `yield_now().await`.
    #[test]
    fn synchronous_loop_starves_neighbor() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            let neighbor_ran = Arc::new(AtomicBool::new(false));

            // Neighbor task: only wants to flip a flag. On a current_thread
            // runtime it is merely *queued* by spawn; it runs only when the
            // main task yields control back to the scheduler.
            let flag = neighbor_ran.clone();
            tokio::spawn(async move {
                flag.store(true, Ordering::SeqCst);
            });

            // A synchronous CPU loop with NO `.await`. coop cannot interrupt it,
            // so the scheduler never gets a chance to run the neighbor.
            let mut acc = 0u64;
            for i in 0..50_000_000u64 {
                acc = acc.wrapping_add(i);
            }
            std::hint::black_box(acc);

            assert!(
                !neighbor_ran.load(Ordering::SeqCst),
                "neighbor must be starved: coop cannot preempt a non-awaiting loop"
            );

            // The only cooperative handoff point in this whole task.
            tokio::task::yield_now().await;

            assert!(
                neighbor_ran.load(Ordering::SeqCst),
                "after an explicit yield the neighbor finally gets to run"
            );
        });
    }
}
