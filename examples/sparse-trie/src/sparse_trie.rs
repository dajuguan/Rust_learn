use std::{
    sync::{mpsc, Arc},
    thread,
};

use crate::{
    errors::StateRootTaskError,
    overlay::OverlayManager,
    proof_task::{ProofResultMessage, ProofWorkerHandle},
};
use alloy_primitives::{
    map::{hash_map::Entry, B256Map},
    B256,
};
use alloy_rlp::{Decodable, Encodable};
use crossbeam_channel::{select_biased, Receiver as CrossbeamReceiver};
use reth_primitives_traits::Account;
use reth_provider::DatabaseProviderROFactory;
use reth_trie::{
    hashed_cursor::HashedCursorFactory, trie_cursor::TrieCursorFactory, DecodedMultiProofV2,
    ProofV2Target, ProofV2TargetParent, TrieAccount, EMPTY_ROOT_HASH, TRIE_ACCOUNT_RLP_MAX_SIZE,
};
use reth_trie_common::{HashedPostState, MultiProofTargetsV2};
use reth_trie_sparse::errors::{SparseStateTrieErrorKind, SparseTrieErrorKind};
use reth_trie_sparse::{LeafUpdate, SparseStateTrie, TrieNodeEpoch};
use tracing::{debug, error, warn};

/// Number of proof targets accumulated during streaming before they are dispatched early, so a
/// long-running update stream does not grow `pending_targets` without bound.
const MAX_PENDING_TARGETS_BEFORE_DISPATCH: usize = 300;

pub struct DefaultStateRootStrategy;

impl DefaultStateRootStrategy {
    pub fn spawn_sparse_trie_task<P>(
        &self,
        provider: P,
        overlay: &OverlayManager,
    ) -> StateRootHandle
    where
        P: DatabaseProviderROFactory<Provider: TrieCursorFactory + HashedCursorFactory + Clone>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let (update_tx, update_rx) = crossbeam_channel::unbounded();
        let (root_tx, root_rx) = mpsc::channel();
        let preserved_trie = match overlay.take_sparse_trie() {
            Some(trie) => trie,
            None => SparseStateTrie::default().with_updates(true),
        };

        let (proof_result_tx, proof_result_rx) = crossbeam_channel::unbounded();
        let proofhandle = ProofWorkerHandle::new(proof_result_tx, provider);
        // Block context this example does not carry: there is no parent header to derive the
        // modification epoch or the parent state root from.
        let new_epoch = TrieNodeEpoch::new(1);
        let parent_state_root = EMPTY_ROOT_HASH;
        let mut task = SparseTrieCacheActor::new(
            update_rx,
            preserved_trie,
            proofhandle,
            root_tx,
            proof_result_rx,
            parent_state_root,
            new_epoch,
        );

        thread::spawn(move || {
            let result = task.run();
            if let Err(err) = &result {
                error!(
                    target: "engine::tree::payload_processor",
                    ?err,
                    "sparse trie task failed"
                );
            }
            let _ = task.root_tx.send(result);
        });

        StateRootHandle { update_tx: Arc::new(SparseTrieStateRootSink::new(update_tx)), root_rx }
    }
}

pub struct StateRootHandle {
    update_tx: Arc<dyn StateRootSink>,
    root_rx: mpsc::Receiver<Result<B256, StateRootTaskError>>,
}

impl StateRootHandle {
    pub fn on_hashed_state_update(&self, state: reth_trie_common::HashedPostState) {
        self.update_tx.on_hashed_state_update(state);
    }

    pub fn on_updates_finished(&self) {
        self.update_tx.on_updates_finished();
    }

    /// Waits for the task outcome, including the reason it refused or failed to compute a root.
    pub fn wait_for_result(&self) -> Result<B256, StateRootTaskError> {
        self.root_rx.recv().map_err(|_| {
            warn!(
                target: "engine::tree::payload_processor",
                "State root sender dropped before publishing a root"
            );
            StateRootTaskError::Canceled
        })?
    }

    /// Waits for the state root, collapsing any task failure into `None`.
    pub fn wait_for_final_root(&self) -> Option<B256> {
        self.wait_for_result().ok()
    }
}

pub trait StateRootSink: Send + Sync + 'static {
    /// Authoritative pre-hashed state update, currently used by BAL streaming.
    fn on_hashed_state_update(&self, state: reth_trie_common::HashedPostState);

    /// Signals that no more authoritative state updates are expected.
    fn on_updates_finished(&self);
}

#[derive(Debug, Clone)]
struct SparseTrieStateRootSink {
    sender: crossbeam_channel::Sender<SparseTrieTaskEvent>,
}

impl SparseTrieStateRootSink {
    const fn new(sender: crossbeam_channel::Sender<SparseTrieTaskEvent>) -> Self {
        Self { sender }
    }
}

impl StateRootSink for SparseTrieStateRootSink {
    fn on_hashed_state_update(&self, state: HashedPostState) {
        let _ = self.sender.send(SparseTrieTaskEvent::HashedState(state));
    }

    fn on_updates_finished(&self) {
        let _ = self.sender.send(SparseTrieTaskEvent::FinishedStateUpdates);
    }
}
/// Sparse trie cache actor
pub struct SparseTrieCacheActor {
    ///////////////////////////////////////////////
    /// inputs
    /// //////////////////////////////////////////
    update_rx: CrossbeamReceiver<SparseTrieTaskEvent>,
    /// The sparse trie state database.
    trie: SparseStateTrie,
    /// The parent block's state root.
    parent_state_root: B256,
    /// The new epoch assigned to nodes modified by this task.
    new_epoch: TrieNodeEpoch,

    ///////////////////////////////////////////////
    /// third party worker
    /// //////////////////////////////////////////
    proof_handle: ProofWorkerHandle,
    proof_result_rx: CrossbeamReceiver<ProofResultMessage>,
    ///////////////////////////////////////////////
    /// internal cached states
    /// //////////////////////////////////////////
    /// Account updates that are tracked until they are applied to the trie. An update is only
    /// removed once the trie accepted it.
    account_updates: B256Map<LeafUpdate>,
    /// Account updates that arrived since the last time they were applied to the trie.
    new_account_updates: B256Map<LeafUpdate>,
    /// already requested proofs to filter out duplicate requests
    requested_proofs: B256Map<ProofV2TargetParent>,
    /// Pending proof targets queued for dispatch to proof workers.
    pending_targets: MultiProofTargetsV2,
    /// Proof batches dispatched to workers and not yet received back.
    in_flight_proof_batches: usize,
    /// Reusable buffer for RLP encoding of accounts.
    account_rlp_buf: Vec<u8>,
    /// Indicates whether all state updates have been received.
    finished_state_updates: bool,

    ///////////////////////////////////////////////
    /// outputs
    /// //////////////////////////////////////////
    /// hashed post state for HashedAccount db
    hashed_post_state: HashedPostState,
    root_tx: mpsc::Sender<Result<B256, StateRootTaskError>>,
}

impl SparseTrieCacheActor {
    pub fn new(
        update_rx: CrossbeamReceiver<SparseTrieTaskEvent>,
        trie: SparseStateTrie,
        proof_handle: ProofWorkerHandle,
        root_tx: mpsc::Sender<Result<B256, StateRootTaskError>>,
        proof_result_rx: CrossbeamReceiver<ProofResultMessage>,
        parent_state_root: B256,
        new_epoch: TrieNodeEpoch,
    ) -> Self {
        let hashed_post_state = HashedPostState::default();
        Self {
            update_rx,
            trie,
            parent_state_root,
            hashed_post_state,
            proof_handle,
            root_tx,
            proof_result_rx,
            requested_proofs: B256Map::default(),
            account_updates: B256Map::default(),
            new_account_updates: B256Map::default(),
            pending_targets: MultiProofTargetsV2::default(),
            in_flight_proof_batches: 0,
            account_rlp_buf: Vec::with_capacity(TRIE_ACCOUNT_RLP_MAX_SIZE),
            finished_state_updates: false,
            new_epoch,
        }
    }

    /// Runs the task until the last state update was received and fully applied to the trie, then
    /// returns the state root. The outcome is published to the handle by the caller.
    pub fn run(&mut self) -> Result<B256, StateRootTaskError> {
        // loop recved states
        let mut done = false;
        while !self.finished_state_updates {
            select_biased!(
                recv(self.update_rx) -> message => {
                    let update = message.map_err(|_| StateRootTaskError::Other(
                        "updates channel disconnected before state root calculation".to_string(),
                    ))?;
                    self.on_update(update);
                }
                recv(self.proof_result_rx) -> message => {
                    let Ok(result) = message else {
                        unreachable!("we own the sender half")
                    };
                    self.on_proof_results(result)?;
                }
            );
            done = self.make_progress()?;
        }

        while !done {
            select_biased!(
                recv(self.proof_result_rx) -> message => {
                    let Ok(result) = message else {
                        unreachable!("we own the sender half")
                    };
                    self.on_proof_results(result)?;
                }
            );
            done = self.make_progress()?;
        }

        debug!(target: "engine::root", "All proofs processed, ending calculation");

        let state_root = match self.trie.root_with_updates(self.new_epoch) {
            Ok((state_root, _trie_updates)) => state_root,
            Err(err)
                if matches!(
                    err.kind(),
                    SparseStateTrieErrorKind::Sparse(SparseTrieErrorKind::Blind)
                ) =>
            {
                // A still-blind account trie means this payload never changed state, so preserve
                // the cached parent root instead of fetching and revealing the unchanged root node.
                self.parent_state_root
            }
            Err(err) => {
                return Err(StateRootTaskError::Other(format!(
                    "could not calculate state root: {err:?}"
                )));
            }
        };

        Ok(state_root)
    }

    fn on_update(&mut self, update: SparseTrieTaskEvent) {
        match update {
            SparseTrieTaskEvent::HashedState(state) => {
                // Storage is out of scope for this example: storage updates are forwarded to the
                // hashed post state but never applied to a storage trie, so every leaf written here
                // carries the empty storage root. `reject_storage_bearing_account` stops the task
                // before such a leaf would overwrite an account that still has storage.
                let account_rlp_buf = &mut self.account_rlp_buf;
                for (&address, &account) in &state.accounts {
                    let encoded =
                        encode_account_leaf_value(account, EMPTY_ROOT_HASH, account_rlp_buf);
                    self.new_account_updates.insert(address, LeafUpdate::Changed(encoded));
                }
                self.hashed_post_state.extend(state);
            }
            SparseTrieTaskEvent::FinishedStateUpdates => {
                self.finished_state_updates = true;
                // All updates have been received, return the finalized hashed state
            }
        }
    }

    /// Coalesces every proof result that is already queued and reveals them in one go, so that
    /// overlapping proofs of the same nodes are revealed in a single trie pass.
    fn on_proof_results(&mut self, message: ProofResultMessage) -> Result<(), StateRootTaskError> {
        let mut result = self.take_proof_result(message)?;
        while let Ok(next) = self.proof_result_rx.try_recv() {
            result.extend(self.take_proof_result(next)?);
        }

        self.trie
            .reveal_decoded_multiproof_v2(result)
            .map_err(|e| StateRootTaskError::Other(format!("could not reveal multiproof: {e:?}")))
    }

    fn take_proof_result(
        &mut self,
        message: ProofResultMessage,
    ) -> Result<DecodedMultiProofV2, StateRootTaskError> {
        let result = message.result?;
        debug_assert!(
            self.in_flight_proof_batches > 0,
            "received proof result without an in-flight proof batch"
        );
        self.in_flight_proof_batches = self.in_flight_proof_batches.saturating_sub(1);
        Ok(result)
    }

    /// Applies buffered updates to the trie and dispatches proof targets.
    ///
    /// Returns `true` once the finish marker was received and all pending trie work is done.
    fn make_progress(&mut self) -> Result<bool, StateRootTaskError> {
        // Messages still queued on the updates channel are handled first, so that applying trie
        // updates and waiting on proofs never delays the ingestion of new state.
        let updates_queued = !self.finished_state_updates && !self.update_rx.is_empty();

        if !updates_queued && self.proof_result_rx.is_empty() {
            // Nothing is queued anywhere, so we can spend the time on applying updates and
            // fetching the proofs they still need.
            self.dispatch_pending_targets();
            self.process_new_updates()?;

            if self.finished_state_updates && !self.has_pending_sparse_trie_updates() {
                return Ok(true);
            }

            self.dispatch_pending_targets();
            self.ensure_not_stalled(updates_queued)?;

            // If there's still nothing queued, spend the time pre-computing the account trie
            // upper hashes.
            if self.proof_result_rx.is_empty() {
                self.trie.calculate_subtries(self.new_epoch);
            }
        } else if !updates_queued {
            // Updates are all known but a proof result is waiting: apply what we have and get the
            // next proof dispatched, then hand back control so the result can be revealed.
            self.process_new_updates()?;
            self.dispatch_pending_targets();
        } else if self.pending_targets.account_targets.len() > MAX_PENDING_TARGETS_BEFORE_DISPATCH {
            // Make sure to dispatch targets if we've accumulated a lot of them.
            self.dispatch_pending_targets();
        }

        Ok(false)
    }

    /// Applies the account updates received since the last call, together with everything the trie
    /// could not accept before, and keeps whatever the trie still refuses in `account_updates`.
    fn process_new_updates(&mut self) -> Result<(), StateRootTaskError> {
        if !self.new_account_updates.is_empty() {
            self.process_account_leaf_updates(true)?;

            for (address, new) in self.new_account_updates.drain() {
                // The newest value always wins over one that is still waiting on a proof.
                self.account_updates.insert(address, new);
            }
        }

        // A proof revealed since the last pass may unblock updates that are already buffered.
        self.process_account_leaf_updates(false)?;

        Ok(())
    }

    /// Stops the task when a leaf it is about to write would drop a non-empty storage root.
    ///
    /// Leaves are always encoded with [`EMPTY_ROOT_HASH`] here, which is only correct for an
    /// account that has no storage, and deleting such an account would leave its storage trie
    /// behind. A buffered update is checked again on every attempt before it reaches
    /// `update_leaves`, so an address whose leaf only becomes readable after a proof is still
    /// caught before the trie accepts the overwrite. Leaves this task wrote itself carry the empty
    /// storage root and pass.
    fn reject_storage_bearing_account(
        trie: &SparseStateTrie,
        address: &B256,
    ) -> Result<(), StateRootTaskError> {
        // Nothing is readable yet, so the trie has not accepted this leaf either way.
        let Some(value) = trie.get_account_value(address) else {
            return Ok(());
        };
        let account = TrieAccount::decode(&mut &value[..]).map_err(|e| {
            StateRootTaskError::Other(format!("invalid account RLP at {address}: {e}"))
        })?;

        if account.storage_root != EMPTY_ROOT_HASH {
            return Err(StateRootTaskError::StorageOutOfScope {
                address: *address,
                storage_root: account.storage_root,
            });
        }

        Ok(())
    }

    /// Invokes `update_leaves` for the accounts trie and collects any new targets.
    ///
    /// Returns whether any updates were drained (applied to the trie).
    fn process_account_leaf_updates(&mut self, new: bool) -> Result<bool, StateRootTaskError> {
        let account_updates =
            if new { &mut self.new_account_updates } else { &mut self.account_updates };

        for address in account_updates.keys() {
            Self::reject_storage_bearing_account(&self.trie, address)?;
        }

        let updates_len_before = account_updates.len();

        self.trie
            .trie_mut()
            .update_leaves(account_updates, |target, parent| {
                match self.requested_proofs.entry(target) {
                    // A parent broader than the one already requested means the cached proof does not
                    // reach deep enough into the trie, so it has to be fetched again.
                    Entry::Occupied(mut entry) => {
                        if parent < *entry.get() {
                            entry.insert(parent);
                            self.pending_targets
                                .account_targets
                                .push(ProofV2Target::new(target).with_parent(parent));
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(parent);
                        self.pending_targets
                            .account_targets
                            .push(ProofV2Target::new(target).with_parent(parent));
                    }
                }
            })
            .map_err(|e| StateRootTaskError::SparseTrie(format!("{e:?}")))?;

        let updates_len_after = account_updates.len();
        debug!(
            target: "engine::tree::payload_processor::sparse_trie",
            applied = updates_len_before - updates_len_after,
            remaining = updates_len_after,
            new,
            "applied account leaf updates"
        );

        Ok(updates_len_after < updates_len_before)
    }

    fn dispatch_pending_targets(&mut self) {
        if self.pending_targets.is_empty() {
            return;
        }

        let targets = core::mem::take(&mut self.pending_targets);
        self.in_flight_proof_batches += 1;
        self.proof_handle.dispatch_account_multiproof(targets, HashedPostState::default());
    }

    fn has_pending_sparse_trie_updates(&self) -> bool {
        !self.new_account_updates.is_empty() || !self.account_updates.is_empty()
    }

    /// Errors when pending trie updates remain but nothing can deliver them: no update messages are
    /// queued, no proof targets are queued or in flight, and no proof results are waiting.
    ///
    /// `updates_queued` is passed in instead of reading `self.update_rx` directly, because in the
    /// draining phase the updates channel is not read anymore and may hold ignored late hints that
    /// must not mask a stall.
    fn ensure_not_stalled(&self, updates_queued: bool) -> Result<(), StateRootTaskError> {
        if self.finished_state_updates
            && !updates_queued
            && self.pending_targets.is_empty()
            && self.in_flight_proof_batches == 0
            && self.proof_result_rx.is_empty()
            && self.has_pending_sparse_trie_updates()
        {
            const MAX_STALLED_PROOF_TARGETS_TO_LOG: usize = 5;

            let mut account_targets = self
                .account_updates
                .keys()
                .map(|target| (*target, self.requested_proofs.get(target).copied()))
                .collect::<Vec<_>>();
            account_targets.sort_unstable();
            let account_targets_truncated =
                account_targets.len().saturating_sub(MAX_STALLED_PROOF_TARGETS_TO_LOG);
            account_targets.truncate(MAX_STALLED_PROOF_TARGETS_TO_LOG);

            error!(
                ?account_targets,
                account_targets_truncated,
                "sparse trie task stalled: pending updates remain but no proof targets are queued or in flight"
            );

            return Err(StateRootTaskError::Stalled);
        }

        Ok(())
    }
}

/// RLP-encodes the account as a [`TrieAccount`] leaf value, or returns empty for deletions.
///
/// `Some(Account::default())` with an empty storage root is encoded as a deletion. This is valid
/// for post-Merge state because EIP-7523 (<https://eips.ethereum.org/EIPS/eip-7523>) prohibits
/// empty accounts. Do not use this encoding rule when replaying historical pre-Merge state, where
/// an empty account and a missing account can have different trie representations.
fn encode_account_leaf_value(
    account: Option<Account>,
    storage_root: B256,
    account_rlp_buf: &mut Vec<u8>,
) -> Vec<u8> {
    if account.is_none_or(|account| account.is_empty()) && storage_root == EMPTY_ROOT_HASH {
        return Vec::new();
    }

    account_rlp_buf.clear();
    account.unwrap_or_default().into_trie_account(storage_root).encode(account_rlp_buf);
    account_rlp_buf.clone()
}

pub enum SparseTrieTaskEvent {
    /// A hashed state update ready to be processed.
    HashedState(HashedPostState),
    /// Signals that all state updates have been received.
    FinishedStateUpdates,
}
