# self-reference结构除了async也存在，为什么只有futures必须限定Pin<&mut self>？
- 因为async的代码，编译器会自动生成自引用代码，开发者无法自行控制，所以需要在编译器层面保证安全，其他则由开发者自行保证安全？
- poll如何被调用？
    - 在runtime通过executor统一poll futures，并且通过waker来唤醒线程避免不必要的调度

# 形象理解async await
- async/await会生成如下状态机的语法糖:

    - async fn => future
    - fut.await => 
    ```
    loop {
        match Future::poll(Pin::new(&mut fut), cx) {
            Poll::Ready(val) => break val,
            Poll::Pending => {
                // send control to executor
                yield_to_executor();
            }
        }
    }
    ```

# 经典案例：把 future 移出 `select!` 以避免被取消（cancellation safety）
- 代码见 [src/select_cancellation.rs](src/select_cancellation.rs)，源自 Base 主网宕机复盘与[修复 PR](https://github.com/base/base/pull/3805)。
- 问题：`select!` 中**某个分支完成时，其余分支正在 poll 的 future 会被直接 drop**。如果 reset future 是**内联在分支里现造的**，那么每当 admin 查询先完成，这个尚在 in-flight（请求已发出、响应未回）的 reset future 连同它的 response receiver 一起被丢弃 → reset 静默失败、永远追不上 tip。
- 修复：把 future 的**所有权移出 `select!`**，分支里只 poll 一个 `&mut` 借用。drop 一个 `&mut F` 不会动到 `F`，future 得以跨循环存活。
  - 方案 1（Base）：`Option<BoxFuture>` —— 堆分配、类型擦除，带 `as_mut().expect(...)` panic 路径。
  - 方案 2（更地道）：`Fuse::terminated()` + `tokio::pin!` —— 栈上 pin、`Pin::set` 原地覆写、无堆分配、无 unwrap。
- 关键点：`pin!` 把同一具体类型的 future 钉在固定栈槽，`Pin::set` 在原地销毁旧值并构造新值，所以每次重试**复用同一块栈存储**；而 `Box::pin` 每次重试都重新 `malloc`。
- 运行：`cargo test -p future_async select_cancellation`（验证 buggy 版重复发出多次 reset，两个 fix 版恰好只发出一次）。

## References
- [Pin in Rust: The Why and How of Immovable Memory](https://dev.to/arichy/pin-in-rust-the-why-and-how-of-immovable-memory-481b)
- [Async/Await- writing os in Rust](https://os.phil-opp.com/async-await/#cooperative-multitasking-1)