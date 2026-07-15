//! Hand-rolled `Future::poll` = an explicit scheduler.
//!
//! Instead of spawning one task per event source and letting tokio interleave
//! them, reth multiplexes ALL sources inside ONE hand-written `poll` and decides
//! everything itself. Writing `poll` by hand is what buys the control — this
//! file shows the THREE knobs it gives you, all built on the same core:
//! *count your own budget, `wake_by_ref` to self-reschedule, and re-initialize
//! locals every poll.*
//!
//!   1. **Fairness** — a per-stream budget so a flooded stream can't starve a
//!      small one (mod `fairness`). Mirrors reth's `poll_nested_stream_with_budget!`
//!      in `crates/net/network/src/transactions/mod.rs`.
//!   2. **Priority** — poll ORDER is the scheduling policy; `continue`-restart
//!      re-checks the important source first (mod `priority`). Mirrors
//!      `crates/net/network/src/swarm.rs:315-352`.
//!   3. **Yielding a CPU-heavy section** — the coop budget can't see pure CPU,
//!      so give the CPU loop its OWN budget and hand-yield (mod `cpu_budget`).
//!      Mirrors `crates/net/network/src/session/active.rs:643-651`.
//!
//! Why one task and not N: in reth all these sources mutate ONE `&mut State`
//! (peers map, tx fetcher, pool imports, ...). Keeping it a single hand-polled
//! task means zero locks — borrow-checker-enforced exclusivity. Splitting into N
//! tasks would force `Arc<Mutex<...>>` everywhere.
//!
//! All demos run on `new_current_thread()` for deterministic, observable
//! single-threaded scheduling.

// ===========================================================================
// 1. FAIRNESS — per-stream budget
// ===========================================================================

/// Multiplex several always-ready streams in one poll, each with its OWN budget,
/// and `wake_by_ref` to reschedule. A flooded stream A cannot starve a small
/// stream B, because each stream's budget is a fresh local every poll.
#[cfg(test)]
mod fairness {
    use std::future::poll_fn;
    use std::task::Poll;

    use tokio::sync::mpsc;

    /// Per-stream budgets, deliberately different (cf. reth's 2 / 10 / 40).
    const BUDGET_A: usize = 10;
    const BUDGET_B: usize = 3;

    /// The single piece of state all streams share and mutate via `&mut`.
    /// In reth this is the peers map, transaction fetcher, pool imports, etc.
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
                // `maybe_more` = did any stream stop only because its budget ran
                // out (still Ready)? Same role as reth's `maybe_more_*` flags.
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
                println!("[fairness] drained A={this_a}, B={this_b}");

                if maybe_more {
                    // A ready stream registered no waker, so we must reschedule
                    // ourselves; then yield so sibling tasks get a turn.
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                // Both streams fully drained -> the whole future is done.
                Poll::Ready(())
            })
            .await;

            state
        })
    }

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
            assert!(a <= BUDGET_A, "A per round must not exceed its fresh budget");
            assert!(b <= BUDGET_B, "B per round must not exceed its fresh budget");
        }
        // 25 items / budget 10 -> at least 3 rounds needed.
        assert!(state.trace.len() >= 3);
    }
}

// ===========================================================================
// 2. PRIORITY — poll order is the scheduling policy
// ===========================================================================

/// Poll HIGH, then MID, then LOW. A lower tier is only touched when every higher
/// tier produced nothing this sweep, so the poll ORDER *is* the priority. As
/// soon as an item is serviced we `continue` back to the TOP, re-checking HIGH
/// first — HIGH is fully drained before MID/LOW make any progress. A `progress`
/// flag prevents busy-looping: a sweep that serviced nothing returns `Pending`.
#[cfg(test)]
mod priority {
    use std::future::poll_fn;
    use std::task::Poll;

    use tokio::sync::mpsc;

    /// Which priority lane an item came from — recorded so tests can assert order.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Prio {
        High,
        Mid,
        Low,
    }

    /// Drain three priority-ordered channels through one hand-rolled poll loop.
    /// Returns the trace of every item serviced, in servicing order.
    fn run_scheduler(fill_high: usize, fill_mid: usize, fill_low: usize) -> Vec<(Prio, u64)> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            let (tx_high, mut rx_high) = mpsc::unbounded_channel::<u64>();
            let (tx_mid, mut rx_mid) = mpsc::unbounded_channel::<u64>();
            let (tx_low, mut rx_low) = mpsc::unbounded_channel::<u64>();

            for i in 0..fill_high as u64 {
                tx_high.send(i).unwrap();
            }
            for i in 0..fill_mid as u64 {
                tx_mid.send(i).unwrap();
            }
            for i in 0..fill_low as u64 {
                tx_low.send(i).unwrap();
            }
            // Drop every sender so the receivers eventually report `Ready(None)`
            // and the hand-rolled future can finally reach `Poll::Ready`.
            drop(tx_high);
            drop(tx_mid);
            drop(tx_low);

            let mut trace: Vec<(Prio, u64)> = Vec::new();

            poll_fn(|cx| {
                loop {
                    // `progress`   == did THIS sweep service an item?
                    // `all_closed` == has every source reported `Ready(None)`?
                    let mut progress = false;
                    let mut all_closed = true;

                    // --- HIGH: polled first => highest priority ---
                    match rx_high.poll_recv(cx) {
                        Poll::Ready(Some(item)) => {
                            println!("[priority] processed HIGH item {item}");
                            trace.push((Prio::High, item));
                            progress = true;
                        }
                        Poll::Pending => all_closed = false,
                        Poll::Ready(None) => {}
                    }

                    // --- MID: only reached if HIGH produced nothing this sweep ---
                    if !progress {
                        match rx_mid.poll_recv(cx) {
                            Poll::Ready(Some(item)) => {
                                println!("[priority] processed MID  item {item}");
                                trace.push((Prio::Mid, item));
                                progress = true;
                            }
                            Poll::Pending => all_closed = false,
                            Poll::Ready(None) => {}
                        }
                    }

                    // --- LOW: only if HIGH and MID are both idle ---
                    if !progress {
                        match rx_low.poll_recv(cx) {
                            Poll::Ready(Some(item)) => {
                                println!("[priority] processed LOW  item {item}");
                                trace.push((Prio::Low, item));
                                progress = true;
                            }
                            Poll::Pending => all_closed = false,
                            Poll::Ready(None) => {}
                        }
                    }

                    // continue-restart: serviced one item, jump back to the TOP
                    // and re-check HIGH before any lower tier gets another turn.
                    if progress {
                        continue;
                    }

                    if all_closed {
                        // Every source closed and drained -> the future is done.
                        println!("[priority] all sources drained -> Ready");
                        return Poll::Ready(());
                    }

                    // At least one source is still open but had nothing ready.
                    // Every `poll_recv` that returned `Pending` registered our
                    // waker, so yield instead of spinning (busy-loop guard).
                    return Poll::Pending;
                }
            })
            .await;

            trace
        })
    }

    fn count(trace: &[(Prio, u64)], prio: Prio) -> usize {
        trace.iter().filter(|(p, _)| *p == prio).count()
    }

    #[test]
    fn priority_order_and_conservation() {
        let (h, m, l) = (4usize, 3usize, 5usize);
        let trace = run_scheduler(h, m, l);
        println!("trace: {trace:?}");

        // Conservation: every item from every source serviced, none lost.
        assert_eq!(count(&trace, Prio::High), h, "all HIGH items serviced");
        assert_eq!(count(&trace, Prio::Mid), m, "all MID items serviced");
        assert_eq!(count(&trace, Prio::Low), l, "all LOW items serviced");
        assert_eq!(trace.len(), h + m + l, "total conserved, no dupes/drops");

        // Priority + continue-restart: HIGH fully drained before MID, MID before
        // LOW. So last HIGH precedes first MID, and last MID precedes first LOW.
        let last_high = trace.iter().rposition(|(p, _)| *p == Prio::High).unwrap();
        let first_mid = trace.iter().position(|(p, _)| *p == Prio::Mid).unwrap();
        let last_mid = trace.iter().rposition(|(p, _)| *p == Prio::Mid).unwrap();
        let first_low = trace.iter().position(|(p, _)| *p == Prio::Low).unwrap();

        assert!(last_high < first_mid, "every HIGH item before any MID item");
        assert!(last_mid < first_low, "every MID item before any LOW item");

        // Concretely: the first `h` entries are exactly the HIGH items.
        assert!(
            trace[..h].iter().all(|(p, _)| *p == Prio::High),
            "the prefix of the trace must be exactly the HIGH items"
        );
    }

    #[test]
    fn empty_sources_ready_immediately() {
        // With nothing to do and all senders dropped, the future still
        // terminates cleanly (returns Ready) rather than hanging or spinning.
        let trace = run_scheduler(0, 0, 0);
        assert!(trace.is_empty(), "no items, empty trace");
    }
}

// ===========================================================================
// 3. CPU BUDGET — the coop budget can't see pure CPU, so budget it yourself
// ===========================================================================

/// tokio's coop budget (128) is only charged when a poll touches a coop-aware
/// resource (a channel's `poll_recv`, IO, ...). It CANNOT see pure CPU work. So
/// a big compute chunk between two await points never triggers a coop yield and
/// starves every other task on a single-threaded executor.
///
/// reth's fix (`crates/net/network/src/session/active.rs:643-651`, which cites
/// the tokio preemption blog): give the CPU-heavy loop its OWN small budget; do
/// at most `BUDGET` units per poll, then `wake_by_ref` + `Pending` to hand-yield.
/// The self-wake is mandatory — a poll that only did CPU registered no waker.
#[cfg(test)]
mod cpu_budget {
    use std::future::poll_fn;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Poll;

    /// Total units of CPU work the worker must complete.
    const TOTAL: usize = 40;
    /// Units the worker may do per poll before it must hand-yield.
    const BUDGET: usize = 4;

    /// Simulate the CPU cost of ONE unit of work (e.g. decoding one message).
    /// A busy compute loop, NOT a sleep — the whole point is that this is pure
    /// CPU that the coop budget cannot account for.
    fn do_one_unit() {
        let mut acc = 0u64;
        for i in 0..200_000u64 {
            acc = acc.wrapping_add(i.wrapping_mul(2_654_435_761));
        }
        std::hint::black_box(acc);
    }

    struct Run {
        work_done: usize,
        worker_yields: usize,
        neighbor_ticks: usize,
    }

    /// Run a CPU-heavy `worker` alongside a `neighbor` on a single-threaded
    /// runtime. With `manual_budget` the worker does at most `BUDGET` units per
    /// poll then hand-yields, so the neighbor runs in every gap. Without it the
    /// worker crams all `TOTAL` units into one poll — coop can't preempt pure
    /// CPU, so the neighbor is starved to zero.
    fn run(manual_budget: bool) -> Run {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            // Structure matters on current_thread: the worker is the block_on
            // ROOT future (awaited directly), the neighbor is spawned. Whenever
            // the root returns Pending, block_on services the queue (running the
            // neighbor) before re-polling the root. Spawning the worker and
            // awaiting its JoinHandle instead would let its self-wake be re-polled
            // ahead of the neighbor, starving it and defeating the demo.
            let ticks = Arc::new(AtomicUsize::new(0));
            let neighbor_ticks = ticks.clone();
            tokio::spawn(async move {
                loop {
                    let k = neighbor_ticks.fetch_add(1, Ordering::SeqCst);
                    println!("  [neighbor] tick {k}");
                    tokio::task::yield_now().await;
                }
            });

            let worker_yields = Arc::new(AtomicUsize::new(0));
            let yields_counter = worker_yields.clone();
            let mut done = 0usize;
            let work_done = poll_fn(move |cx| {
                // Fresh per-poll budget (re-initialized every poll, like reth).
                let mut budget = BUDGET;
                while done < TOTAL {
                    do_one_unit(); // pure CPU — coop charges nothing for this
                    done += 1;

                    if manual_budget {
                        budget -= 1;
                        if budget == 0 {
                            // Budget spent, work remains, no waker registered
                            // (we only did CPU). MUST self-wake, then yield.
                            let n = yields_counter.fetch_add(1, Ordering::SeqCst);
                            println!(
                                "[cpu] budget exhausted, yielding (done {done}/{TOTAL}, yield #{n})"
                            );
                            cx.waker().wake_by_ref();
                            return Poll::Pending;
                        }
                    }
                }
                println!("[cpu] all work done ({done}/{TOTAL})");
                Poll::Ready(done)
            })
            .await;

            Run {
                work_done,
                worker_yields: worker_yields.load(Ordering::SeqCst),
                neighbor_ticks: ticks.load(Ordering::SeqCst),
            }
        })
    }

    #[test]
    fn manual_budget_lets_neighbor_run() {
        let r = run(true);
        println!(
            "work_done={}, worker_yields={}, neighbor_ticks={}",
            r.work_done, r.worker_yields, r.neighbor_ticks
        );

        assert_eq!(r.work_done, TOTAL, "worker must finish all its work");

        let expected_yields = TOTAL / BUDGET; // 40 / 4 = 10

        // Neighbor not starved: it ran ~once per worker yield (slack for jitter).
        assert!(
            r.neighbor_ticks >= expected_yields - 1,
            "neighbor should run ~once per worker yield, got {} (expected >= {})",
            r.neighbor_ticks,
            expected_yields - 1
        );
    }

    #[test]
    fn no_manual_yield_starves_neighbor() {
        // Counter-example: without the manual budget the worker crams all TOTAL
        // units into one poll and never hand-yields. coop charges nothing for the
        // busy loop, so the scheduler never runs the neighbor. Starved to zero.
        let r = run(false);
        println!(
            "work_done={}, worker_yields={}, neighbor_ticks={}",
            r.work_done, r.worker_yields, r.neighbor_ticks
        );

        assert_eq!(r.work_done, TOTAL, "worker still finishes its work");
        assert_eq!(r.worker_yields, 0, "worker never hand-yields");
        assert_eq!(
            r.neighbor_ticks, 0,
            "neighbor is starved: coop cannot preempt the pure-CPU loop"
        );
    }
}
