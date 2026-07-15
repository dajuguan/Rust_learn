//! Batch fan-out with `recv_many`: amortize per-message network overhead by
//! draining a whole burst in one shot and fanning it out as a SINGLE batch.
//!
//! This is the async/`.await` counterpart to reth's `poll_recv_many` path in
//! `TransactionsManager` (crates/net/network/src/transactions/mod.rs). Same
//! goal — turn O(txs) fan-outs into O(bursts) — but here we are allowed to
//! `.await`, so we use `recv_many` instead of hand-rolling `poll_recv_many`.
//!
//! Key idea: the producer dumps a whole burst into the channel at once and then
//! sleeps. By the time the consumer's `recv_many` runs, the entire burst is
//! already queued, so one call drains it all and we fan out once per burst
//! rather than once per transaction.
//!
//! The producer uses a BOUNDED channel + `try_send`: it never blocks on a full
//! buffer, it sheds load (drops) instead. This is the canonical `try_send` use
//! case — a best-effort path where dropping under overload beats stalling the
//! producer. (`unbounded_channel` has no `try_send`: it can never be full, so
//! its `send` is already synchronous and non-blocking.)

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

/// Max hashes packed into one fan-out, mirroring reth's broadcast soft limit
/// (`SOFT_LIMIT_COUNT_HASHES_IN_NEW_POOLED_TRANSACTIONS_BROADCAST_MESSAGE`).
const FANOUT_BATCH_LIMIT: usize = 128;

/// One fan-out of a batch. In reth this packs the hashes into a single
/// `NewPooledTransactionHashes` message per peer; here we just log the size.
fn fan_out(batch: &[u64]) {
    println!("[fan-out] one message for a batch of {} tx(s)", batch.len());
}

/// Consumer: block until at least one tx is queued, then drain the whole burst
/// (up to the limit) in a single `recv_many` and fan it out once. Returns the
/// size of every batch it fanned out.
async fn consumer(mut rx: mpsc::Receiver<u64>) -> Vec<usize> {
    let mut batch_sizes = Vec::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // Waits until >=1 is ready, then moves up to the limit into `buf`.
        // Returns 0 only once all senders are dropped and the channel is empty.

        // only consumes one coop budget, cause it pops all the ready items in one go, and then yields to other tasks
        let n = rx.recv_many(&mut buf, FANOUT_BATCH_LIMIT).await;
        if n == 0 {
            break;
        }
        fan_out(&buf);
        batch_sizes.push(n);
    }
    batch_sizes
}

/// Producer: emit `bursts` random-sized bursts (1..=256 txs each), sleeping
/// between bursts so the consumer sees each burst as one queued batch. Uses a
/// fixed-seed LCG so the demo needs no `rand` dependency and stays deterministic.
///
/// Sends with `try_send`: bursts can exceed the channel capacity, and rather
/// than `.await` for room (backpressure) the producer DROPS the overflow. This
/// is load-shedding. Returns `(accepted, dropped)`.
async fn producer(tx: mpsc::Sender<u64>, bursts: usize, seed: u64) -> (usize, usize) {
    let mut state = seed;
    let (mut accepted, mut dropped) = (0usize, 0usize);
    for _ in 0..bursts {
        // LCG (Numerical Recipes constants); take high bits for the burst size.
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let count = 1 + (state >> 33) % 256; // 1..=256

        for i in 0..count {
            match tx.try_send(i) {
                Ok(()) => accepted += 1,
                // Buffer full: shed load instead of blocking the producer.
                Err(TrySendError::Full(_)) => dropped += 1,
                // Receiver gone: no point continuing.
                Err(TrySendError::Closed(_)) => return (accepted, dropped),
            }
        }
        println!("[producer] burst of {count}: accepted so far {accepted}, dropped {dropped}");
        // Yield time so the consumer wakes and drains the buffer as one batch.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // `tx` dropped on return -> consumer's recv_many eventually returns 0.
    (accepted, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small bounded buffer so bursts (up to 256) overflow it and `try_send`
    /// visibly sheds load.
    const CHANNEL_CAP: usize = 64;

    #[tokio::test]
    async fn batches_amortize_fanout() {
        let bursts = 10;
        let (tx, rx) = mpsc::channel::<u64>(CHANNEL_CAP);

        let consumer = tokio::spawn(consumer(rx));
        let (accepted, dropped) = producer(tx, bursts, 0x9E37_79B9_7F4A_7C15).await;
        let batch_sizes = consumer.await.unwrap();

        println!(
            "accepted = {accepted}, dropped = {dropped}, fan-outs = {}, sizes = {batch_sizes:?}",
            batch_sizes.len()
        );

        // Everything that got INTO the channel is fanned out; nothing accepted
        // is lost (dropped txs never entered the channel).
        assert_eq!(accepted, batch_sizes.iter().sum::<usize>());

        // try_send actually shed load: bursts exceeded the buffer capacity, so
        // the producer dropped the overflow instead of blocking.
        assert!(
            dropped > 0,
            "try_send should have dropped overflow on a full buffer"
        );

        // Far fewer fan-outs than transactions: one message per burst, not per tx.
        assert!(
            batch_sizes.len() <= bursts,
            "should fan out at most once per burst, not once per tx"
        );
        assert!(
            batch_sizes.len() < accepted,
            "batching must produce fewer messages than transactions"
        );

        // A bounded buffer of CHANNEL_CAP can never hand recv_many more than
        // that in one batch (and never more than the broadcast soft limit).
        assert!(
            batch_sizes
                .iter()
                .all(|&n| n <= CHANNEL_CAP.min(FANOUT_BATCH_LIMIT))
        );
    }
}
