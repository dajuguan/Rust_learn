//! Reth-style scheduling: multiplex several always-ready streams inside ONE
//! task, each with its own budget, and manually `wake_by_ref` to reschedule.
//!
//! This mirrors `NetworkManager` / `TransactionsManager` in reth:
//!   - all streams mutate ONE `&mut State` — no locks, borrow-checker-enforced
//!     exclusivity (the reason it stays a single task instead of N tasks);
//!   - each stream gets an independent per-poll budget (fairness between
//!     streams: a flooded stream A cannot starve a small stream B);
//!   - "budget" is just a local re-initialized every poll — no persistent
//!     counter to reset;
//!   - if any stream stopped only because its budget ran out (it was still
//!     Ready), we MUST self-wake, because a `Ready` poll registered no waker.

use std::future::poll_fn;
use std::task::Poll;

use tokio::sync::mpsc;

/// Per-stream budgets, deliberately different (cf. reth's 2 / 10 / 40).
const BUDGET_A: usize = 10;
const BUDGET_B: usize = 3;

/// The single piece of state all streams share and mutate via `&mut`.
/// In reth this is the peers map, transaction fetcher, pool imports, etc.
#[cfg(test)]
#[derive(Default)]
struct State {
    processed_a: usize,
    processed_b: usize,
    /// Per-poll trace: (items of A this round, items of B this round).
    /// Proves the interleaving / fairness between the two streams.
    trace: Vec<(usize, usize)>,
}

/// Drain both pre-filled channels through a single hand-rolled multiplexing
/// poll loop, returning the shared state (including the per-round trace).
#[cfg(test)]
fn run_scheduler(fill_a: usize, fill_b: usize) -> State {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    rt.block_on(async {
        let (tx_a, mut rx_a) = mpsc::unbounded_channel::<u64>();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel::<u64>();
        for i in 0..fill_a as u64 {
            tx_a.send(i).unwrap();
        }
        for i in 0..fill_b as u64 {
            tx_b.send(i).unwrap();
        }
        drop(tx_a);
        drop(tx_b);

        let mut state = State::default();

        poll_fn(|cx| {
            // `maybe_more` = did any stream stop only because its budget ran out
            // (i.e. it was still Ready)? Same role as reth's `maybe_more_*` flags.
            let mut maybe_more = false;
            let mut this_a = 0usize;
            let mut this_b = 0usize;

            // --- stream A: fresh budget every poll (this is the "reset") ---
            let mut budget_a = BUDGET_A;
            loop {
                match rx_a.poll_recv(cx) {
                    Poll::Ready(Some(_)) => {
                        state.processed_a += 1; // shared &mut, no lock
                        this_a += 1;
                        budget_a -= 1;
                        if budget_a == 0 {
                            maybe_more = true; // still Ready, we chose to stop
                            break;
                        }
                    }
                    Poll::Ready(None) | Poll::Pending => break,
                }
            }

            // --- stream B: its OWN fresh budget, unaffected by A's flood ---
            let mut budget_b = BUDGET_B;
            loop {
                match rx_b.poll_recv(cx) {
                    Poll::Ready(Some(_)) => {
                        state.processed_b += 1;
                        this_b += 1;
                        budget_b -= 1;
                        if budget_b == 0 {
                            maybe_more = true;
                            break;
                        }
                    }
                    Poll::Ready(None) | Poll::Pending => break,
                }
            }

            state.trace.push((this_a, this_b));
            println!("[poll] drained A={this_a}, B={this_b}");

            if maybe_more {
                // A ready stream registered no waker, so we must reschedule
                // ourselves; then yield so sibling tasks get a turn.
                cx.waker().wake_by_ref();
                println!("[poll] waiting for next poll (yielding to sibling tasks)");
                return Poll::Pending;
            }
            // Both streams fully drained -> the whole future is done.
            Poll::Ready(())
        })
        .await;

        state
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_stream_budget_is_fair() {
        // A is flooded (25), B is small (8). Naive "one shared budget" would let
        // A eat everything before B runs; per-stream budgets must not.
        let state = run_scheduler(25, 8);
        println!("trace: {:?}", state.trace);

        assert_eq!(state.processed_a, 25);
        assert_eq!(state.processed_b, 8);

        // Fairness: in EVERY round where A still had a backlog (A hit its full
        // budget of 10), B also made progress that same round — A never starved
        // B despite having far more work queued.
        for &(a, b) in &state.trace {
            if a == BUDGET_A {
                assert!(
                    b > 0,
                    "stream B must make progress in the same poll A is flooding"
                );
            }
        }
    }

    #[test]
    fn budget_resets_every_poll() {
        // Each round drains at most BUDGET_A from A and BUDGET_B from B, proving
        // the budget is re-initialized (not carried over) on every poll.
        let state = run_scheduler(25, 8);
        for &(a, b) in &state.trace {
            assert!(
                a <= BUDGET_A,
                "A per round must not exceed its fresh budget"
            );
            assert!(
                b <= BUDGET_B,
                "B per round must not exceed its fresh budget"
            );
        }
        // 25 items / budget 10 -> at least 3 rounds needed.
        assert!(state.trace.len() >= 3);
    }
}
