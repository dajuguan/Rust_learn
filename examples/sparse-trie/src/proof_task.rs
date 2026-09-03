//! Parallel proof computation using a single account worker thread.
//!
//! # Architecture
//!
//! - **Account Worker**: A single pre-spawned worker with a dedicated database transaction
//!   that handles account multiproof computation.
//! - **Direct Channel Access**: `ProofWorkerHandle` provides a type-safe queue method with direct
//!   access to the account worker channel, eliminating routing overhead.
//! - **Automatic Shutdown**: The worker terminates gracefully when all handles are dropped.
//!
//! # Message Flow
//!
//! 1. The `SparseTrieCacheTask` prepares an account job and hands it to
//!    `ProofWorkerHandle`. The job carries a `ProofResultContext` so the worker knows how to send
//!    the result back.
//! 2. The worker receives the job, runs the proof, and sends a `ProofResultMessage` through the
//!    provided `ProofResultSender`.
//! 3. The `SparseTrieCacheTask` receives the message and proceeds with its state-root logic.
//!
//! ```text
//! SparseTrieCacheTask -> ProofWorkerHandle -> Account Worker
//!        ^                       |
//!        |                       v
//! ProofResultMessage <-- ProofResultSender
//! ```

use std::{
    thread::spawn,
    time::{Duration, Instant},
};

use crossbeam_channel::{unbounded, Sender as CrossbeamSender};
use reth_provider::DatabaseProviderROFactory;
use reth_trie::{
    hashed_cursor::HashedCursorFactory,
    proof_v2::{self, SyncAccountValueEncoder},
    trie_cursor::TrieCursorFactory,
    DecodedMultiProofV2, HashedPostState, MultiProofTargetsV2,
};

use crate::errors::StateRootTaskError;

/// Message containing a completed proof result with metadata for direct delivery to
/// `SparseTrieCacheTask`.
#[derive(Debug)]
pub struct ProofResultMessage {
    /// The proof calculation result
    pub result: Result<DecodedMultiProofV2, StateRootTaskError>,
    /// Time taken for the entire proof calculation (from dispatch to completion)
    pub elapsed: Duration,
    /// Original state update that triggered this proof
    pub state: HashedPostState,
}

/// Context for sending proof calculation results back to `SparseTrieCacheTask`.
///
/// This struct contains all context needed to send and track proof calculation results.
/// Workers use this to deliver completed proofs back to the main event loop.
#[derive(Debug, Clone)]
pub struct ProofResultContext {
    /// Channel sender for result delivery
    pub sender: CrossbeamSender<ProofResultMessage>,
    /// Original state update that triggered this proof
    pub state: HashedPostState,
    /// Calculation start time for measuring elapsed duration
    pub start_time: Instant,
}

impl ProofResultContext {
    /// Creates a new proof result context.
    pub const fn new(
        sender: CrossbeamSender<ProofResultMessage>,
        state: HashedPostState,
        start_time: Instant,
    ) -> Self {
        Self { sender, state, start_time }
    }
}

/// Input parameters for account multiproof computation.
#[derive(Debug)]
pub struct AccountMultiproofInput {
    /// The targets for which to compute the multiproof.
    pub targets: MultiProofTargetsV2,
    /// Context for sending the proof result.
    pub proof_result_sender: ProofResultContext,
}

impl AccountMultiproofInput {
    /// Returns the [`ProofResultContext`] for this input, consuming the input.
    fn into_proof_result_sender(self) -> ProofResultContext {
        self.proof_result_sender
    }
}

/// Internal message for the account worker.
#[derive(Debug)]
struct AccountWorkerJob {
    /// Account multiproof input parameters.
    /// Boxed to avoid large stack copy/allocations for the potentially large `targets` field.
    input: Box<AccountMultiproofInput>,
}

/// A handle that provides type-safe access to the proof worker pool.
///
/// The handle stores a direct sender to the account worker pool.
/// All handles share reference-counted channels, and the worker shuts down
/// gracefully when all handles are dropped (channel closes).
#[derive(Debug, Clone)]
pub struct ProofWorkerHandle {
    /// Channel sender for receiving proof results
    result_tx: CrossbeamSender<ProofResultMessage>,
    /// Direct sender to the account worker pool
    account_work_tx: CrossbeamSender<AccountWorkerJob>,
}

impl ProofWorkerHandle {
    /// Spawns an account worker with a dedicated database transaction.
    ///
    /// The worker runs until all handles are dropped (channel closes).
    ///
    /// # Parameters
    /// - `result_tx`: Channel for sending proof results back to the caller
    /// - `factory`: Factory for creating database providers with trie/hash cursors
    pub fn new<Factory>(result_tx: CrossbeamSender<ProofResultMessage>, factory: Factory) -> Self
    where
        Factory: DatabaseProviderROFactory<Provider: TrieCursorFactory + HashedCursorFactory + Clone>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let (account_work_tx, account_work_rx) = unbounded::<AccountWorkerJob>();
        // Clone result_tx so the closure and Self each have their own sender.
        let worker_result_tx = result_tx.clone();

        // Spawn account worker on a dedicated OS thread.
        // The worker creates its own provider from the factory, then loops
        // processing account multiproof jobs until the channel closes.
        let _ = spawn(move || {
            // Each worker gets its own provider (and thus its own DB transaction).
            let provider = match factory.database_provider_ro() {
                Ok(p) => p,
                Err(e) => {
                    let _ = worker_result_tx.send(ProofResultMessage {
                        result: Err(StateRootTaskError::Provider(e.to_string())),
                        elapsed: Duration::ZERO,
                        state: Default::default(),
                    });
                    return;
                }
            };

            while let Ok(job) = account_work_rx.recv() {
                let input = *job.input;
                let AccountMultiproofInput { mut targets, proof_result_sender } = input;

                let proof_start = Instant::now();
                let result = compute_account_multiproof(&provider, &mut targets);
                let total_elapsed = proof_result_sender.start_time.elapsed();

                let ProofResultContext { sender: result_tx, state, .. } = proof_result_sender;
                let _ =
                    result_tx.send(ProofResultMessage { result, elapsed: total_elapsed, state });

                tracing::trace!(
                    target: "trie::proof_task",
                    proof_time_us = proof_start.elapsed().as_micros(),
                    total_elapsed_us = total_elapsed.as_micros(),
                    "Account multiproof completed"
                );
            }
        });

        Self { result_tx, account_work_tx }
    }

    /// Dispatch an account multiproof computation.
    ///
    /// The result will be sent via the `result_tx` channel included in this handle.
    pub fn dispatch_account_multiproof(
        &self,
        targets: MultiProofTargetsV2,
        state: HashedPostState,
    ) {
        let proof_result_sender =
            ProofResultContext::new(self.result_tx.clone(), state, Instant::now());
        let input = AccountMultiproofInput { targets, proof_result_sender };
        let job = AccountWorkerJob { input: Box::new(input) };
        let _ = self.account_work_tx.send(job);
    }
}

/// Compute a V2 account multiproof for the given targets using the provided provider.
///
/// This function uses a [`SyncAccountValueEncoder`] which synchronously computes storage roots
/// for each account leaf encountered during the trie walk. The storage root is computed by
/// creating a fresh `StorageProofCalculator` per account on demand.
///
/// Since storage proofs are not dispatched to a separate worker pool, only account-level
/// proof nodes are included in the returned [`DecodedMultiProofV2`]. The `storage_proofs`
/// map will be empty.
///
/// # Arguments
///
/// * `provider` - A provider implementing both [`TrieCursorFactory`] and [`HashedCursorFactory`]
/// * `targets` - The [`MultiProofTargetsV2`] specifying which accounts to generate proofs for
///
/// # Returns
///
/// A [`DecodedMultiProofV2`] containing account trie proof nodes, or a [`StateRootTaskError`]
/// if the proof computation fails.
pub fn compute_account_multiproof<Provider>(
    provider: &Provider,
    targets: &mut MultiProofTargetsV2,
) -> Result<DecodedMultiProofV2, StateRootTaskError>
where
    Provider: TrieCursorFactory + HashedCursorFactory + Clone,
{
    let MultiProofTargetsV2 { account_targets, storage_targets: _ } = targets;

    // Create account-level trie cursors for the proof calculator.
    let account_trie_cursor =
        provider.account_trie_cursor().map_err(|e| StateRootTaskError::Provider(e.to_string()))?;
    let account_hashed_cursor = provider
        .hashed_account_cursor()
        .map_err(|e| StateRootTaskError::Provider(e.to_string()))?;

    // Create a sync account value encoder that computes storage roots on demand. The encoder owns
    // its cursor factories and wraps them in `Rc` internally, so we hand it cloned providers rather
    // than a reference. Each account leaf lazily opens storage cursors from these factories when
    // its storage root is encoded.
    let mut value_encoder = SyncAccountValueEncoder::new(provider.clone(), provider.clone());

    // Create the V2 proof calculator for the account trie. The value-encoder type parameter is
    // inferred from the `proof` call below as `SyncAccountValueEncoder<Provider, Provider>`.
    let mut calculator = proof_v2::ProofCalculator::new(account_trie_cursor, account_hashed_cursor);

    // Run the proof calculation.
    // ProofCalculator::proof() resets cursors, walks the account trie,
    // and for each leaf uses the value_encoder to get the (possibly deferred) RLP encoding.
    // The SyncAccountValueEncoder::DeferredEncoder synchronously computes the storage root
    // when encode() is called, using a fresh StorageProofCalculator per account.
    let account_proofs = calculator
        .proof(&mut value_encoder, account_targets)
        .map_err(|e| StateRootTaskError::Other(e.to_string()))?;

    // Storage proofs are not computed in this simplified version.
    // In the full reth implementation, storage proofs are dispatched to a separate
    // worker pool and collected asynchronously. Here we skip that complexity.
    let storage_proofs = Default::default();

    Ok(DecodedMultiProofV2 { account_proofs, storage_proofs })
}
