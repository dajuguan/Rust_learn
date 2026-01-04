mod node_core {
    use std::fmt::Debug;

    pub trait DBProvider {
        fn get(&self, key: &str) -> String;
        fn set(&self, key: &str, val: String) -> Option<String>;
    }

    pub trait NodeTypes {
        type Block: Debug + Copy + From<u32>;
        type ChainSpec;
        type Provider: DBProvider;
    }

    pub struct BuildContext<N: NodeTypes> {
        provider: N::Provider,
    }

    impl<N: NodeTypes> BuildContext<N> {
        pub fn build_provider(provider: N::Provider) -> Self {
            BuildContext { provider }
        }
    }

    pub trait EvmExecutor<N: NodeTypes>: Send + Sync {
        fn handle_block(&self, block_number: N::Block);
    }

    pub trait MockConsensus<N: NodeTypes>: Send + Sync {
        fn validate_block(&self, block_number: N::Block) -> bool;
    }

    pub trait NodeComponentsTrait<T: NodeTypes>: Send + Sync {
        type Consensus: MockConsensus<T>;
        type Evm: EvmExecutor<T>;

        fn consensus(&self) -> &Self::Consensus;
        fn evm(&self) -> &Self::Evm;
    }

    pub struct NodeComponents<Consensus, EVM> {
        consensus: Consensus,
        evm: EVM,
    }

    impl Default for NodeComponents<(), ()> {
        fn default() -> Self {
            Self {
                consensus: (),
                evm: (),
            }
        }
    }

    impl<Node, Consensus, EVM> NodeComponentsTrait<Node> for NodeComponents<Consensus, EVM>
    where
        Node: NodeTypes,
        Consensus: MockConsensus<Node>,
        EVM: EvmExecutor<Node>,
    {
        type Consensus = Consensus;
        type Evm = EVM;

        fn consensus(&self) -> &Self::Consensus {
            &self.consensus
        }

        fn evm(&self) -> &Self::Evm {
            &self.evm
        }
    }

    impl<Consensus, EVM> NodeComponents<Consensus, EVM> {
        pub fn build_consensus<CB>(self, consensus: CB) -> NodeComponents<CB, EVM> {
            let Self { consensus: _, evm } = self;
            NodeComponents { consensus, evm }
        }

        pub fn build_executor<EB>(self, evm: EB) -> NodeComponents<Consensus, EB> {
            let Self { consensus, evm: _ } = self;
            NodeComponents { consensus, evm }
        }
    }

    pub trait NodeComponetsBuilder<Node: NodeTypes> {
        type Components: NodeComponentsTrait<Node>;

        /// returns the created components.
        fn build_components(&self, ctx: &BuildContext<Node>) -> Self::Components;
    }

    pub struct NodeAdapter<N: NodeTypes, C> {
        types: N,
        components: C,
    }

    impl<N, C> NodeAdapter<N, C>
    where
        N: NodeTypes,
        C: NodeComponentsTrait<N>,
    {
        /// simulate backfill
        fn back_fill(&self) {
            println!("historical sync done!")
        }
        /// Simulate engine api for processing a block: validate + handle
        fn process_block(&self, block_number: N::Block) {
            if self.components.consensus().validate_block(block_number) {
                self.components.evm().handle_block(block_number);
            } else {
                println!("Block {:?} is invalid!", block_number);
            }
        }

        pub fn new(types: N, components: C) -> Self {
            Self { types, components }
        }

        /// inherit tons of core logics for free!
        pub fn launch_node(&self) {
            self.back_fill();
            self.process_block(0.into());
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

    pub struct EthConsensus;
    impl MockConsensus<EthTypes> for EthConsensus {
        fn validate_block(&self, block_number: <EthTypes as NodeTypes>::Block) -> bool {
            println!("[ETH] Validating block {}", block_number);
            block_number % 2 == 0 // dummy rule
        }
    }

    pub struct EthEvm;
    impl EvmExecutor<EthTypes> for EthEvm {
        fn handle_block(&self, block_number: <EthTypes as NodeTypes>::Block) {
            println!("[ETH] Handling block {}", block_number);
        }
    }

    /// ETH components implementing consensus + block handler
    pub struct EthNode {
        ctx: BuildContext<EthTypes>,
    }

    impl EthNode {
        pub fn new() -> Self {
            Self {
                ctx: BuildContext::build_provider(MockProvider),
            }
        }

        pub fn ctx(&self) -> &BuildContext<EthTypes> {
            &self.ctx
        }
    }
    impl EthNode {
        pub fn components(&self) -> NodeComponents<EthConsensus, EthEvm> {
            NodeComponents::default()
                .build_consensus(EthConsensus {})
                .build_executor(EthEvm {})
        }
    }

    impl NodeComponetsBuilder<EthTypes> for EthNode {
        type Components = NodeComponents<EthConsensus, EthEvm>;
        fn build_components(&self, ctx: &BuildContext<EthTypes>) -> Self::Components {
            self.components()
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
        type Block = u64;
        type ChainSpec = String;
        type Provider = MockProvider;
    }

    pub struct BscConsensus;
    impl MockConsensus<BscTypes> for BscConsensus {
        fn validate_block(&self, block_number: <BscTypes as NodeTypes>::Block) -> bool {
            println!("[BSC] Validating block {}", block_number);
            block_number % 2 == 0 // dummy rule
        }
    }

    pub struct BscEvm;
    impl EvmExecutor<BscTypes> for BscEvm {
        fn handle_block(&self, block_number: <BscTypes as NodeTypes>::Block) {
            println!("[BSC] Handling block {}", block_number);
        }
    }

    /// ETH components implementing consensus + block handler
    pub struct BscNode {
        ctx: BuildContext<BscTypes>,
    }

    impl BscNode {
        pub fn new() -> Self {
            Self {
                ctx: BuildContext::build_provider(MockProvider),
            }
        }

        pub fn ctx(&self) -> &BuildContext<BscTypes> {
            &self.ctx
        }
    }
    impl BscNode {
        pub fn components(&self) -> NodeComponents<BscConsensus, BscEvm> {
            NodeComponents::default()
                .build_consensus(BscConsensus {})
                .build_executor(BscEvm {})
        }
    }

    impl NodeComponetsBuilder<BscTypes> for BscNode {
        type Components = NodeComponents<BscConsensus, BscEvm>;
        fn build_components(&self, ctx: &BuildContext<BscTypes>) -> Self::Components {
            self.components()
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
        let node = EthNode::new();
        let eth_node = NodeAdapter::new(EthTypes, node.build_components(node.ctx()));
        eth_node.launch_node();

        // Compose BSC node
        let node = BscNode::new();
        let bsc_node = NodeAdapter::new(BscTypes, node.build_components(node.ctx()));
        bsc_node.launch_node();
    }
}
