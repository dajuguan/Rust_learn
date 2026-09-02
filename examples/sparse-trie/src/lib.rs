mod db;
mod overlay;
mod sparse_trie;

mod test {
    use crate::overlay::OverlayManager;

    #[test]
    fn sparse_trie_task_works() {
        let trie_overlay = OverlayManager::default();
        let strategy = crate::sparse_trie::DefaultStateRootStrategy;
        let handle = strategy.spawn_sparse_trie_task(&trie_overlay);

        // send some hashed state updates
        let state = reth_trie_common::HashedPostState::default();
        handle.on_hashed_state_update(state);

        // finish updates
        handle.on_updates_finished();
        // wait for the final state
        let final_root = handle.wait_for_final_root();
        println!("Final state root: {:?}", final_root);
    }
}
