use std::sync::{mpsc, Arc, Mutex};

use crate::overlay::OverlayManager;
use alloy_primitives::B256;
use crossbeam_channel::Receiver as CrossbeamReceiver;
use reth_trie_common::{HashedPostState, MultiProofTargetsV2};
use reth_trie_sparse::{RevealableSparseTrie, SparseStateTrie};
use tracing::{debug, warn};

pub struct DefaultStateRootStrategy;

impl DefaultStateRootStrategy {
    pub fn spawn_sparse_trie_task(&self, overlay: &OverlayManager) -> StateRootHandle {
        let (update_tx, update_rx) = crossbeam_channel::unbounded();
        let (root_tx, root_rx) = mpsc::channel();
        let preserved_trie = match overlay.take_sparse_trie() {
            Some(trie) => trie,
            None => SparseStateTrie::default().with_updates(true),
        };

        let task = SparseTrieCacheActor::new(update_rx, preserved_trie, root_tx);

        task.run();
        StateRootHandle { update_tx: Arc::new(SparseTrieStateRootSink::new(update_tx)), root_rx }
    }
}

pub struct StateRootHandle {
    update_tx: Arc<dyn StateRootSink>,
    root_rx: mpsc::Receiver<B256>,
}

impl StateRootHandle {
    pub fn on_hashed_state_update(&self, state: reth_trie_common::HashedPostState) {
        self.update_tx.on_hashed_state_update(state);
    }

    pub fn on_updates_finished(&self) {
        self.update_tx.on_updates_finished();
    }

    pub fn wait_for_final_root(&self) -> B256 {
        match self.root_rx.recv() {
            Ok(root) => root,
            Err(_) => {
                warn!(
                    target: "engine::tree::payload_processor",
                    "State root sender dropped, returning zero root"
                );
                B256::ZERO
            }
        }
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
    state: SparseStateTrie,

    ///////////////////////////////////////////////
    /// internal state updates
    /// //////////////////////////////////////////
    /// hashed post state for HashedAccount db

    ///////////////////////////////////////////////
    /// outputs
    /// //////////////////////////////////////////
    /// hashed post state for HashedAccount db
    hashed_post_state: HashedPostState,
    root_tx: mpsc::Sender<B256>,
    // h
}

impl SparseTrieCacheActor {
    pub fn new(
        update_rx: CrossbeamReceiver<SparseTrieTaskEvent>,
        state: SparseStateTrie,
        root_tx: mpsc::Sender<B256>,
    ) -> Self {
        let hashed_post_state = HashedPostState::default();
        Self { update_rx, state, hashed_post_state, root_tx }
    }
    pub fn run(&self) {
        // loop recved states
        // todo!("loop recved states and update sparse trie, then send root to root_tx");
        // send root to root_tx
        let final_root = B256::ZERO; // Placeholder for the final root calculation
        if self.root_tx.send(final_root).is_err() {
            warn!(
                target: "engine::tree::payload_processor",
                "State root receiver dropped, dropping trie"
            );
        }
    }
}

pub enum SparseTrieTaskEvent {
    /// A hashed state update ready to be processed.
    HashedState(HashedPostState),
    /// Prefetch proof targets (passed through directly).
    PrefetchProofs(MultiProofTargetsV2),
    /// Signals that all state updates have been received.
    FinishedStateUpdates,
}
