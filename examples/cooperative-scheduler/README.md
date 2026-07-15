# Tokio 协作式调度(Cooperative Scheduling)—— Talk 材料

一句话主旨:**tokio 是 cooperative 而非 preemptive 的 —— 让出(yield)只发生在 `.await` 点,且是你的责任。** tokio 用一个 128 的 coop budget 兜底,但真实系统里的公平、优先级、批量、背压,最终都要你自己设计。本 repo 用一组最小可跑的实验把这条主线讲清楚,并落到 reth 网络层的真实代码。

所有实验都是 `#[test]`,一条命令跑全:

```bash
cargo test                          # 跑全部
cargo test <name> -- --nocapture    # 看某个 demo 的 trace 输出
```

> 所有 demo 刻意用 `new_current_thread()` 单线程 runtime —— 行为确定、易观测。生产环境默认是 multi-thread work-stealing 调度器(讲的时候要点明这个差异)。

---

## 整场叙事顺序(讲的时候照着走)

一条主线贯穿:**cooperative 意味着让出是你的责任 → tokio 给了 128 兜底 → 但真实系统要自己接管调度**。

1. **钩子**:同步循环卡死服务(`synchronous_loop_starves_neighbor`)——为什么 tokio 不抢占?
2. **模型**:async = cooperative;让出 = 在 `.await` 点返回 `Pending`;什么会卡死 poll。
3. **coop budget 128**:tokio 的兜底补丁(`yields_every_128_polls`)+ 按操作计费的细节。
4. **逃生舱**:`unconstrained` 关掉兜底(`unconstrained_disables_the_budget`);长计算的正解(手动让出 / `spawn_blocking`)。
5. **reth 手写 poll = 你就是调度器**(一个文件三种控制):公平 → 优先级 → 让 CPU。
6. **背压**:由「手写 poll 里收发都不能阻塞」这条约束,引出收 / 发的三种做法(见下面的关系图)。
7. **收尾**:心智模型。

第 6 步的逻辑关系(reserve_send 的位置就在这里):

```
手写 poll 里「不能 .await、不能阻塞」这条约束
        │
        ├── 收(consume):poll_recv_many 攒批            → batch_fanout.rs
        │
        └── 发(produce):满了怎么办?
              ├── 可丢:try_send 削峰(load-shedding)   → batch_fanout.rs
              └── 必达:poll_reserve + send_item(预留)  → reserve_send.rs
```

所以 `reserve_send` 不是孤立的一招,而是「发送」这条线上和 `try_send` 并列的另一半:**同样不能阻塞,但语义从『可丢』变成『必达』**。讲的时候紧接 `batch_fanout` 的 `try_send` 之后引出,最自然。

---

## 文件 → 知识点映射(4 个文件)

| 文件 | 知识点 | 对应章节 |
|---|---|---|
| [src/tokio_yield.rs](src/tokio_yield.rs) | preemption 反例 / coop budget 128 / `unconstrained` | 开场 + 第一、二、三幕 |
| [src/reth_multistream_scheduler.rs](src/reth_multistream_scheduler.rs) | 手写 poll = 显式调度器:**公平 / 优先级 / 让 CPU** 三种控制 | 第四幕 |
| [src/batch_fanout.rs](src/batch_fanout.rs) | `recv_many` 攒批 fan-out + `try_send` 削峰 | 第四幕 + 背压 |
| [src/reserve_send.rs](src/reserve_send.rs) | `poll_reserve` + `send_item` 两段式必达发送 | 背压 |

---

## 开场钩子:为什么我的 async 服务卡死了?

> 一个 task 陷入纯计算循环,整个服务无响应 —— 但 CPU 只有一个核在转。

`tokio_yield::synchronous_loop_starves_neighbor`:单线程 runtime 上,一段**不含 `.await` 的同步循环**把邻居 task 彻底饿死,直到显式 `yield_now().await` 才让出。

抛出问题:**为什么 tokio 不像操作系统那样直接抢占它?**

---

## 第一幕:模型 —— 为什么会这样

async = cooperative multitasking,对比 OS 的 preemptive:

- **Preemptive**:调度器可在任意时刻(时钟中断)打断执行单元,无需其配合。
- **Cooperative**(tokio 及几乎所有 Rust async runtime):运行时只能在 `.await` 点拿回控制权。两个 `.await` 之间的代码,运行时无能为力。

根因:`async fn` 编译成状态机,运行时通过调用 `Future::poll` 推进它。`poll` 是普通同步函数调用 —— 进去后控制权就在它手里,直到它自己 `return`(`Ready` 或 `Pending`)。tokio 没有中断线程、强夺栈的能力。

**核心口诀:让出 = 在 `.await` 点返回 `Poll::Pending`。**

`poll_fn` 是最朴素的 Future:每次 `poll` 就是调用一次闭包,`Ready` / `Pending` / `wake` 三件套。本 repo 所有手写 poll 都基于它。

**澄清:什么会「卡死 poll」,什么不会。** 手写 `poll` 里:

- **会卡死**:`.await`(非 async 的 poll 里编译不过)、忙 spin(`loop { ... Pending => continue }`,永不让出)、`block_on` / `blocking_send` / `blocking_recv`(真阻塞 OS 线程)。
- **不会卡死(是让出)**:返回 `Poll::Pending`,以及 `ready!(e)` 宏 —— 它等价于 `match e { Ready(v)=>v, Pending=>return Poll::Pending }`,遇到 `Pending` 就**优雅让出**,不是阻塞。后面 `poll_reserve` + `send_item` 那套不用裸 `ready!`,原因不是 ready! 会阻塞,而是①消息要跨 poll 保活(裸 ready! 会把已 `take` 出的消息 drop 丢掉)、②想继续跑同一 poll 里的其它活。

---

## 第二幕:tokio 的补丁 —— coop budget(128)

问题:一个「一直就绪、从不 `Pending`」的 task(比如收一个永远有数据的 channel)会怎样?它形式上一直在 `.await`,但从不让出,照样饿死别人。

tokio 的解法:每个 task 每次被 poll 时获得一个 **budget = 128**。每次 poll 一个 coop-aware 资源(channel `poll_recv`、IO……)成功就扣 1;扣到 0 时,资源即使数据就绪也强制返回 `Pending`,并唤醒自身,下次 poll 预算重填 128。

`tokio_yield::yields_every_128_polls`:塞满一个 unbounded channel,手写 poll 循环 `poll_recv`,数出**每一轮正好 128** 次后被强制让出:

```
coop yield after 128 recvs in this round
  [neighbor] got a turn to run (tick 0)
...
per-round recv counts: [128, 128, 128, 10]; neighbor ran 3 time(s)
```

邻居 task 的打印**严格交错在每次 coop yield 之后** —— 正是这 128 次让出给了邻居运行窗口。

**关键细节:budget 按「操作次数」计费,不按「item 数」。** 单个 `poll_recv` 每 item 扣 1(→ 128 个后让出);而 `recv_many` / `poll_recv_many` 一次操作搬一整批(源码里只在开头 `poll_proceed` 扣 1 点),所以一次能搬 >128 个而不被截断。这解释了 reth 为什么用 `poll_recv_many(limit=4096)`:coop 不会把它腰斩,真正限批的是业务的 soft limit。

参考:https://tokio.rs/blog/2020-04-preemption

---

## 第三幕:逃生舱 —— 协作式的代价与对策

### `unconstrained`:关掉 128 兜底

`tokio_yield::unconstrained_disables_the_budget`:把同样的 always-ready drain 包进 `tokio::task::coop::unconstrained(...)`,128 失效 —— 394 个 item **一次 poll 全清、零让出**:

```
unconstrained rounds: [394]     # 对比未关时的 [128, 128, 128, 10]
```

证明 128 是可选补丁,不是铁律;贪心的 `unconstrained` future 又会饿死邻居。

### 长计算 / 阻塞怎么办

开场钩子的正解:**长计算要么手动插让出点(`yield_now` / `consume_budget` / 自建 budget),要么丢给 `spawn_blocking`。** 其中「自建 budget」的做法在第四幕 reth 的 `cpu_budget` 子模块里有完整 demo。

---

## 第四幕:真实世界 —— reth 手写 poll = 显式调度器

reth 的 `NetworkManager` / `TransactionsManager` 都是 **endless future**(`poll` 永远返回 `Pending`),一次 poll 里轮询多条流。**为什么用一个手写 poll 的 task,而不是每条流一个 task?** 因为所有流操作同一份 `&mut State`(peers map、tx fetcher、pool imports……),单 task 里是**无锁**的 `&mut self`;拆成 N 个 task 就得上 `Arc<Mutex>`,引入锁竞争、死锁、样板。

手写 `poll` 本身就是「你亲手当调度器」。[src/reth_multistream_scheduler.rs](src/reth_multistream_scheduler.rs) 用三个子模块展示它给你的三种控制,内核都一样:**自己数 budget + `wake_by_ref` 自唤醒 + 每 poll 局部变量满血重置**。

### 控制一:公平(per-stream budget)—— `mod fairness`

问题:任务级单计数器(coop 的 128)给不了「子流之间的公平」——一条洪水流会独吞预算,饿死同一 poll 里的其它流。

reth 给每条流独立预算(见 reth `crates/net/network/src/transactions/mod.rs` 的 `poll_nested_stream_with_budget!`,取值 2 / 10 / 40 按单 item 成本反比设定)。

`per_stream_budget_is_fair`(A 积压 25、B 只有 8,预算 10 / 3):

```
[fairness] drained A=10, B=3     # A 打满预算(还有积压),B 同轮照样推进
[fairness] drained A=10, B=3
[fairness] drained A=5,  B=2
```

要点:① budget 是每 poll 重新初始化的局部变量,不存在「手动重置」;② budget 打满时那条流最后是 `Ready`(没注册 waker),所以必须 `wake_by_ref` 自唤醒,否则 endless future 假死。

### 控制二:优先级(poll 顺序)—— `mod priority`

per-stream 预算管公平,poll 顺序管优先级。reth 的 swarm(见 reth `crates/net/network/src/swarm.rs:315-352`)按优先级轮询多个事件源,某源有进展就 `continue` 回顶部重查高优先级,用 `progress` 标志防 busy-loop。

`priority_order_and_conservation`(HIGH/MID/LOW):

```
[priority] processed HIGH item 0
... 全部 HIGH ...
[priority] processed MID  item 0
... 全部 MID ...
[priority] processed LOW  item 0
... 全部 LOW ...
[priority] all sources drained -> Ready
```

要点:**轮询顺序就是调度策略**;`continue`-restart 给高优先级插队权;`progress` 布尔量检测「本轮是否有前进」,无前进才返回 `Pending`(此时所有 `Pending` 的 poll 都已注册 waker,不空转)。

### 控制三:让出 CPU 密集段 —— `mod cpu_budget`

coop budget 只对「碰 coop-aware 资源的 poll」计费,**数不到纯 CPU 计算**。一段重解码 / 重计算即使夹在 channel poll 之间,也不会触发 coop 让出。

reth 的做法(见 reth `crates/net/network/src/session/active.rs:643-651`,注释直接引了 preemption 博客):给 CPU 密集的 loop **自己设一个小 budget**,做满 N 个单位就 `cx.waker().wake_by_ref()` + `return Poll::Pending` 主动让出。自唤醒是**强制**的 —— 只做了 CPU 活的 poll 没注册任何 waker。

`manual_budget_lets_neighbor_run`(`TOTAL=40`, `BUDGET=4`):

```
[cpu] budget exhausted, yielding (done 4/40, yield #0)
  [neighbor] tick 0
[cpu] budget exhausted, yielding (done 8/40, yield #1)
  [neighbor] tick 1
...
work_done=40, worker_yields=10, neighbor_ticks=10
```

对照测试 `no_manual_yield_starves_neighbor`:worker 一口气做完 40 个不让出 → `neighbor_ticks == 0`,直接印证「coop 数不到纯 CPU」。这正是开场钩子的正解落地。

---

## 背压专题(呼应引子)

talk 的起因是一个真实背压问题:reth 某个 stream 处理过久 → 下游 `poll_recv_many` 下次攒的批过大 → `try_send` 直接丢了部分交易(入站/出站速率不匹配)。

总纲(接上面的关系图):**手写 poll 里收发都不能阻塞**,于是收用 `poll_recv_many` 攒批,发按「可丢 / 必达」分成 `try_send` 与 `poll_reserve`+`send_item` 两条。下面三小节就是这棵树的三个叶子。

### 攒批 fan-out + 削峰 —— `batch_fanout.rs`

reth 手写 poll 里**收发都不能阻塞**。于是:

- **收**:`poll_recv_many(limit=4096)` 一次攒一批,把 O(txs) 次 fan-out 降为 O(bursts) 次(每批打进一条 gossip 消息)。
- **发**:满了不能等,`try_send` 削峰(丢弃),或 unbounded channel。

`batches_amortize_fanout`(bounded `channel(64)` + `try_send`,producer 突发 1..=256):

```
accepted = 510, dropped = 728, fan-outs = 10, sizes = [17, 64, 38, 64, 64, 64, 7, 64, 64, 64]
```

10 个突发只产生 10 次 fan-out;缓冲满时 `try_send` 丢弃过载(load-shedding),生产者从不阻塞。

### 两段式非阻塞「必达」发送:`poll_reserve` + `send_item` —— `reserve_send.rs`

有时一条关键消息(比如「我要终止了」)**既不能丢、又不能阻塞**。`try_send` 会丢,`send().await` 会阻塞。解法是 `tokio_util::sync::PollSender`(见 reth `crates/net/network/src/session/active.rs:608-625`):先 `poll_reserve(cx)` 在 poll 语义下**先占座**(满则 `Pending` 并注册 waker),拿到 `Ready(Ok)` 后 `send_item` **入座、保证成功**。

`critical_message_is_reserved_then_delivered_exactly_once`(`channel(2)` 先填满):

```
[sender] channel full, cannot reserve yet, will retry    # 满,占座失败,Pending 走人
[consumer] received Prefill(0)                           # consumer 腾出空位
[consumer] received Prefill(1)
[sender] reserved a slot, critical message delivered     # 占到座 -> send_item 必达
[consumer] received Critical("SHUTDOWN")
```

critical 消息经历一次 reserve 失败后必达、恰好一次、sender 全程不阻塞。

### 发送家族全景(三种「满了怎么办」)

| 写法 | 满了的行为 | 语义 | 能否用在手写 poll |
|---|---|---|---|
| `send().await`(bounded) | 挂起等空位 | 背压,零丢失 | 否(要 `.await`) |
| `try_send`(bounded) | 立即 `Err(Full)` | 削峰,可丢 | 是 |
| `reserve().await` → `permit.send` | 挂起等 permit | 预留必达 | 否 |
| `poll_reserve` + `send_item`(PollSender) | 返回 `Pending`,下次再试 | 预留必达,不阻塞 | 是 |
| `unbounded_channel` 的 `send` | 永不满 | 同步、非阻塞、零丢失(代价:无上界) | 是 |

判断是否 async 的口诀:**这操作有没有可能需要等待?** 等空位 / 等消息 / 等 IO / 等锁 → async(`.await`);「试一下不行就算了」→ 同步 `try_*`。

### 两种背压范式

| | 丢弃式(shedding) | 等待式(blocking) |
|---|---|---|
| 原语 | `try_send` 满了丢 | `Semaphore::acquire().await` / `send().await` |
| 何时用 | 数据可丢(通知、遥测、gossip);热路径 / 持锁 / 手写 poll 不能等 | 数据不能丢但能等;想让上游自动限速 |
| 代价 | 丢数据 | 上游被挂起(可能死锁,如双向 channel 互等) |

reth 的经验:**分层背压** —— 关键 / 低频 / 幂等的控制消息给独立的无背压逃生通道,数据平面走 bounded 并允许 load-shedding;已验证交易本体绝不走丢弃路径。

---

## 收尾:一句话心智模型

> **`.await` = 「我可能要在这儿等一会儿」;cooperative 意味着让出是你的责任。**

tokio 给了 128 兜底,但重活(`spawn_blocking` / manual budget)、公平(per-stream budget)、优先级(poll 顺序)、批量(`recv_many`)、背压(`try_send` / `poll_reserve`)—— 最终都要你自己设计。reth 就是把这套亲手实现了一遍。

---

## reth 代码参考索引(repo-root-relative)

- `crates/net/network/src/manager.rs:1104-1170` —— NetworkManager endless future + budget
- `crates/net/network/src/transactions/mod.rs:1534-1717` —— TransactionsManager 多流 poll + `poll_recv_many` 攒批
- `crates/net/network/src/budget.rs` —— `poll_nested_stream_with_budget!` 宏 + 各预算常量
- `crates/net/network/src/swarm.rs:315-352` —— 手写 poll 的优先级轮询
- `crates/net/network/src/session/active.rs:643-651` —— CPU 密集段的 manual budget + `wake_by_ref`
- `crates/net/network/src/session/active.rs:608-625` —— `poll_reserve` + `send_item` 必达发送
