mod node_core {

    pub trait DBProvider {
        fn get(&self, key: &str) -> String;
        fn set(&self, key: &str, val: String) -> Option<String>;
    }

    pub trait NodeTypes {
        type Block;
        type ChainSpec;
        type Provider: DBProvider;
    }

    pub struct BuildContext<N: NodeTypes> {
        provider: N::Provider,
    }

    pub trait NodeComponents<T: NodeTypes>: Send + Sync {
        fn validate_block(&self, block_number: u64) -> bool;
        fn handle_block(&self, block_number: u64);

        fn bn(&self) -> T::Block;
    }

    pub trait NodeComponetsBuilder<Node: NodeTypes>: Send {
        type Components: NodeComponents<Node>;

        /// returns the created components.
        fn build_components(&self, ctx: BuildContext<Node>) -> Self::Components;
    }

    pub struct NodeAdapter<N: NodeTypes, C> {
        types: N,
        components: C,
    }

    impl<N, C> NodeAdapter<N, C>
    where
        N: NodeTypes,
        C: NodeComponents<N>,
    {
        /// Process a block: validate + handle
        fn process_block(&self, block_number: u64) {
            if self.components.validate_block(block_number) {
                self.components.handle_block(block_number);
            } else {
                println!("Block {} is invalid!", block_number);
            }
        }

        pub fn new(types: N, components: C) -> Self {
            Self { types, components }
        }

        pub fn launch_node(&self) {
            self.process_block(0);
        }
    }

    pub struct MockProvider;
    impl DBProvider for MockProvider {
        fn get(&self, key: &str) -> String {
            println!("get :{}", key);
            "get".to_string()
        }
        fn set(&self, key: &str, val: String) -> Option<String> {
            println!("set,key:{}, val:{}", key, val);
            Some("".to_string())
        }
    }
}

// -----------------------------
// Ethereum chain plugin
// -----------------------------
mod eth {
    use super::node_core::*;

    #[derive(Clone)]
    pub struct EthTypes;
    impl NodeTypes for EthTypes {
        type Block = u64;
        type ChainSpec = String;
        type Provider = MockProvider;
    }

    /// ETH components implementing consensus + block handler
    pub struct EthComponents;

    impl NodeComponents<EthTypes> for EthComponents {
        fn validate_block(&self, block_number: u64) -> bool {
            println!("[ETH] Validating block {}", block_number);
            block_number % 2 == 0 // dummy rule
        }

        fn handle_block(&self, block_number: u64) {
            println!("[ETH] Handling block {}", block_number);
        }

        fn bn(&self) -> <EthTypes as NodeTypes>::Block {
            1
        }
    }

    pub struct EthNodeBuilder;
    impl NodeComponetsBuilder<EthTypes> for EthNodeBuilder {
        type Components = EthComponents;
        fn build_components(&self, ctx: BuildContext<EthTypes>) -> Self::Components {
            EthComponents {}
        }
    }
}

// -----------------------------
// BSC chain plugin
// -----------------------------
mod bsc {
    use super::node_core::*;

    #[derive(Clone)]
    pub struct BscTypes;
    impl NodeTypes for BscTypes {
        type Block = String;
        type ChainSpec = u64;
        type Provider = MockProvider;
    }

    /// BSC components implementing consensus + block handler
    pub struct BscComponents;

    impl NodeComponents<BscTypes> for BscComponents {
        fn validate_block(&self, block_number: u64) -> bool {
            println!("[BSC] Validating block {}", block_number);
            block_number % 2 == 0 // dummy rule
        }

        fn handle_block(&self, block_number: u64) {
            println!("[BSC] Handling block {}", block_number);
        }

        fn bn(&self) -> <BscTypes as NodeTypes>::Block {
            "2".to_string()
        }
    }

    pub struct EthNodeBuilder;
    impl NodeComponetsBuilder<BscTypes> for EthNodeBuilder {
        type Components = BscComponents;
        fn build_components(&self, ctx: BuildContext<BscTypes>) -> Self::Components {
            BscComponents {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_node_builder_modularity() {
        use super::bsc::*;
        use super::eth::*;
        use super::node_core::*;

        // Compose ETH node
        let eth_node = NodeAdapter::new(EthTypes, EthComponents);
        eth_node.launch_node();

        // Compose BSC node
        let bsc_node = NodeAdapter::new(BscTypes, BscComponents);
        bsc_node.launch_node();
    }
}
