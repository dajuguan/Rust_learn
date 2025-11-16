# self-reference结构除了async也存在，为什么只有futures必须限定Pin<&mut self>？
- 因为async的代码，编译器会自动生成自引用代码，开发者无法自行控制，所以需要在编译器层面保证安全，其他则由开发者自行保证安全？
- poll如何被调用？
    - 在runtime通过executor统一poll futures，并且通过waker来唤醒线程避免不必要的调度


## References
- [Pin in Rust: The Why and How of Immovable Memory](https://dev.to/arichy/pin-in-rust-the-why-and-how-of-immovable-memory-481b)
- [Async/Await- writing os in Rust](https://os.phil-opp.com/async-await/#cooperative-multitasking-1)