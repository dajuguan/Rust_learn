trait TraitObj {
    // not allowed, because trait obj's size in trait is not known at compile time.
    // fn return_trait_obj() -> impl RandomTrait;

    // allowed only when RandomTrait satisfy trait object safety.
    fn return_trait_obj() -> Box<dyn RandomTrait>;
}

trait RandomTrait {
    fn done(&self) {}
}

trait TraitNonObjectSafe<const N: usize> {
    const CONFIG: usize;

    // rust运行时会单态化，所以clone_self不是对象安全的，不能作为trait obj
    fn clone_self(&self) -> Self;

    // T 是编译期泛型，不同调用点 T 不同，编译器无法在运行时确定哪一个版本应该放进 vtable. 也不能作为trait object
    fn serialize<T: RandomTrait>(&self, value: &T);
    // 运行时无法通过通过 &self 调用，所以内存中没有相关的值。也不能作为trait object

    fn new();
    // 依赖常量泛型，每一个泛型实现都要单态化monomorphization，导致没法生成统一的vtable

    fn len(&self) -> usize {
        N
    }
    // 依赖关联常量，与常量泛型同理
    fn config(&self) {
        println!("{}", Self::CONFIG);
    }
}

#[test]
fn test_trait_obj() {}
