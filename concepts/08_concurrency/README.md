理解&T不一定是并发安全的，以及什么能刻画并发安全？
- 在 Rust 中，不可变引用并不等价于并发安全。由于内部可变性（interior mutability）的存在，即使只持有 &T，也可能修改内部状态；若该修改不是并发安全的，就可能在多线程环境中产生数据竞争。
- Rust 的并发安全并不是由“可变 / 不可变引用”决定的，而是由 Send / Sync trait 精确刻画的。&T 仅在 T: Sync 时才允许跨线程共享。
    - &T 能否跨线程共享，取决于 T: Sync，而不是 &T 是否可变。
举例:
&Rc<T>不是Send(Rc<T>不是Sync)，看起来也能在多线程安全共享啊，为什么实际上不行呢？
- 因为Rc::clone(&self)通过&self.clone就能改变引用计数，而该引用计数可能会造成数据竞争


## Send, Sync types
| 类型          | Send 条件        | Sync 条件        |
|---------------|------------------|------------------|
| Rc<T>         | ❌               | ❌               |
| &mut T        | T: Send          | ❌               |
| MutexGuard<T> | ❌               | T: Sync          |
| Arc<T>        | T: Send + Sync   | T: Send + Sync   |

为什么Arc<T>: Send 要求T: Send外还能Sync？
- Arc<T> 的本质是 多线程共享所有权的智能指针。
- 当你 deref 一个 Arc<T> 时，你得到的是一个 &T。
- 不是 Deref 语法本身决定的，而是 Arc 的共享语义决定的：Send Arc<T> 必须保证多线程同时访问 &T 时安全，因此要求 T: Sync。