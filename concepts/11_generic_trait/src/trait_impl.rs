/* 
trait 定义里 type EVM: EvmFactory; 规定的是关联类型的最低约束。在 impl 里你给 EVMF 加上额外的 + StricterTrait,只是让 EVMF 满足比要求更严格的条件——只要它仍然满足 EvmFactory(最低要求满足了),编译器就接受。多出来的约束完全合法。 

cd concepts/11_generic_trait && cargo check
*/
pub trait BlockExecutorFactory {
    type EVM: EvmFactory;
}

pub trait EvmFactory {
    fn hello();
}

struct OpBlockExecutorFactory<EVMF> {
    evm_factory: EVMF,
}

pub trait StricterTrait {
    fn stricter_method(&self) -> String;
}

impl<EVMF> BlockExecutorFactory for OpBlockExecutorFactory<EVMF>
where
// 允许更严格的约束
    EVMF: EvmFactory + StricterTrait,
{
    type EVM = EVMF;
}
