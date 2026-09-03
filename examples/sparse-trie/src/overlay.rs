use std::sync::{Arc, Mutex};

use reth_trie_sparse::SparseStateTrie;

// cross block overlays
#[derive(Debug, Default)]
pub struct OverlayManager {
    trie: Arc<Mutex<Option<SparseStateTrie>>>,
}

impl OverlayManager {
    /// Takes the preserved sparse trie if present.
    pub fn take_sparse_trie(&self) -> Option<SparseStateTrie> {
        self.trie.lock().unwrap().take()
    }

    /// Clears the preserved sparse trie when trie task fails to produce a final root.
    pub fn clear_sparse_trie(&self) {
        *self.trie.lock().unwrap() = None;
    }
}
