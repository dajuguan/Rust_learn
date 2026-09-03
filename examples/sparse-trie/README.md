# Account-only Reth TrieDB 模拟器设计规格

状态：Draft v0.3  
目标：用尽可能少的 Rust 代码，模拟 Reth 2.0 中与账户状态有关的核心机制。本文只定义设计与行为，不包含具体 Rust 实现。

v0.3 变更：确定复用策略——trie 内核与全部密码学/编码管道不再自研，直接复用 `alloy-trie` 与 `reth-trie-sparse`（见第 3 节）；proof 合同从 fork 风格的「fault 上报 + `min_len` 局部证明 + 重试循环」改为 upstream 风格的「预派发多重证明（multiproof）+ sequencer 按序 reveal + provider 兜底」。自研范围收缩到调度系统、存储模拟和 driver。

## 1. 目标

这个示例需要把下面几件事串成一条可运行、可验证的链路：

1. 以 `keccak256(address)` 为 key 保存 canonical account state，不保存 plain account state。
2. 从持久化 TrieDB 为目标账户生成 Merkle proof。
3. 把 proof reveal 到只展开局部路径的 sparse trie。
4. 在 sparse trie 中完成账户插入、更新和删除，并计算新 state root。
5. 生成并持久化 trie updates，使 TrieDB 前进到新 root。
6. 跨 block 保留 sparse trie，模拟 sparse trie cache，请求trie的时候分为两层(root trie包括前三层，更深层的用subtrie来表示)。
7. 已请求过 proof 的目标不再重复请求：fetched-target 去重与 EmptyProof 短路，模拟 upstream 的 multiproof 去重语义。
8. 用 plain-address changeset 和 history index 查询 historical account。
9. 支持最小 reorg/unwind，并验证 unwind 后 root 和账户状态都正确。
10. 用单线程、确定性的事件循环模拟 Reth 的 scheduler，明确展示 pending、ready、inflight、completion、drain 和 commit barrier。

最终示例应展示以下完整流程：

```text
plain account changes
        │
        ├── keccak(address) ──> LeafUpdate
        │
        ├── update_leaves()
        |   ├── blind:派生proof targets (根据缓存更新parent)
        |   |   └── fetch_proofs(targets) -> Vec<DecodedMultiProofV2> 
        |   |        (这里是把多个targets的proofs合并去重后返回，这里有优化)
        |   |   └── on_proof_results(DecodedMultiProofV2)
        |   |         └── reveal_decoded_multiproof_v2(DecodedMultiProofV2)
        |   ├── revealed: trie.update_leaves()
        |       └──更新中间的trie节点
        |── subtrie更新完毕
        |   └──finished_state_updates(payload那边不会再发送新交易的事件)
        |   └──&& no_pending_sparse_trie_updates
        │── 更新root + 返回trieupdates
        │
        ├── new state root + TrieUpdates
        │
        └── atomic commit
                ├── HashedAccounts
                ├── TrieDB
                ├── AccountChangeSets
                ├── AccountHistory
                └── BlockMeta
```


### 1.1 Upstream 参照：reth 真实的 `run()` / `make_progress()` 调度

本节记录 reth 上游（2.4.1 时期，`crates/engine/tree/src/tree/state_root_strategy/sparse_trie.rs`）中 `SparseTrieCacheTask` 的实际调度机制，是 8.6/8.8 节模拟策略的原始出处。行号随上游演进会漂移，以函数名为准。

#### 输入通道与消息类型

任务持有两个接收通道，外加一个取消通道：

```text
updates          — SparseTrieTaskMessage:
                     HashedState(HashedPostState)   权威状态更新（执行结果）
                     PrefetchProofs(targets)-> Vec<ProofTrieNodeV2>       prewarm 的 best-effort 预取提示
                     FinishedStateUpdates           输入结束标记
proof_result_rx  — ProofResultMessage               proof worker 回传的多重证明（multiproof）结果
cancel_rx        —                                  取消信号
```

关键点：proof target 不是外部事件。外部只送状态更新和预取提示，target 是 `update_leaves` 撞到 blinded 节点时在内部产生的。

#### `run()` 的两阶段主循环

```text
// 阶段一：流式阶段，直到收到 FinishedStateUpdates
while !finished_state_updates:
    select_biased! {
        recv(updates)          => on_message(...)；pending_updates += 1
        recv(proof_result_rx)  => on_proof_results(...)   // coalesce + reveal
        recv(cancel_rx)        => return Canceled
    }
    done = make_progress()

// 阶段二：排空（draining）阶段，只剩 proof 结果和取消
while !done:
    select_biased! {
        recv(proof_result_rx)  => on_proof_results(...)
        recv(cancel_rx)        => return Canceled
    }
    done = make_progress()

// 收尾：一次性算最终 root
root_with_updates(new_epoch)
```

值得注意的语义：

1. `select_biased!` 使 `updates` 通道优先于 proof 结果——先吸收输入，避免生产者阻塞；
2. 收到的状态更新被 `on_hashed_state_update` 立刻摊平进 `new_account_updates: B256Map<LeafUpdate>`——leaf 在这一步就用 `EMPTY_ROOT_HASH` 编码完毕，storage 更新只进 `hashed_post_state` 而不建 storage trie；`pending_updates` 只是"有几条消息还没被 apply"的计数器，不存数据；
3. proof 结果有合并（coalesce）优化：`on_proof_results` 收到一条后会把通道里已排队的其余结果全部 `try_recv` 出来合成一个大 multiproof，再一次性 `reveal_decoded_multiproof_v2`，降低每次 reveal 的固定开销（对应 8.6 节 policy 2）；
4. 收尾时如果 account trie 仍是 blind（整个 block 没改任何状态），直接沿用 parent state root，跳过为算 root 而取 proof。
5. 因为 leaf 里的 storage root 恒为 `EMPTY_ROOT_HASH`，`process_account_leaf_updates` 在把 update 交给 trie 之前会先看一眼该地址当前可读的 leaf：若它带着非空 storage root，任务以 `StorageOutOfScope` 失败，而不是把别人的存储指针清掉。这个检查在每轮重试时都会重做，所以 proof reveal 之后才变得可读的 leaf 同样拦得住——被更新的账户一律无 storage，这是本示例的边界条件，不是实现漏掉的优化。

#### `make_progress()`：背压优先、乐观应用

```text
fn make_progress():
    updates_queued = !finished && !updates.is_empty()

    if !updates_queued && proof_result_rx.is_empty():
        // 两个通道都空 —— 做完整的一轮重活
        dispatch_pending_targets()          // 把攒下的 target 发给 proof workers
        process_new_updates()               // 乐观 apply：本轮新 update + 上轮被退回的，见下
        if finished && 无 pending 更新: return true   // 唯一的终止出口
        dispatch_pending_targets()
        ensure_not_stalled()                // 卡死检测：还有活但没有任何在途事件 → 硬错误
        if proof_result_rx.is_empty():
            calculate_subtries(new_epoch)   // 真正空闲：预算干净子树的 hash（对应 8.6 policy 7）
    else if !updates_queued:
        process_new_updates()               // 只有 proof 结果在排队：apply 后赶紧回去收
        dispatch_pending_targets()
    else if pending_targets.len() > chunk_size:
        dispatch_pending_targets()          // 输入还在涌入：只在攒多时顺手派发（对应 8.6 policy 4）
    return false
```

核心是「先乐观应用、缺什么再要什么」，与直觉的「先取 proof 再更新」相反：

1. `process_new_updates` → `trie.update_leaves(updates, callback)`：能用已缓存节点完成的 leaf 更新当场做掉（warm cache 的常态，计入 cache hit 指标）；
2. 撞到 blinded 的 update 触发 callback，经 `fetched_account_targets: B256Map<ProofV2TargetParent>` 去重——同 key 只有在需要更宽的 parent 覆盖（`parent < entry`，`NONE` 最宽）时才重新请求——然后攒进 `pending_targets`，update 本体被退回 map 等下一轮；
3. `dispatch_pending_targets` 按 `chunk_size` 切块派发（对应 8.6 policy 5/6 的 chunk 语义）；
4. proof 结果回来 → reveal → 被退回的 update 在下一次 `process_new_updates` 中补完。

背压设计：只要任一输入通道非空，`make_progress` 就不做重活，尽快回到 `select` 收消息；空闲时反而主动找活干（预算子树 hash），把最终 `root_with_updates` 的临界路径工作提前摊掉。

一句话总结：**优先吸收状态更新摊平成 leaf update map → 空闲时乐观批量 apply，缺节点的 update 退回并去重后批量请求 proof → proof 结果合并 reveal → 退回的 update 下轮补完 → 结束标记后排空在途 proof，一次性算 root。**

## 2. 非目标

为了把实现规模限制在教学示例范围内，本项目不实现：

- storage trie；
- `SLOAD`、`SSTORE`、storage root 和 storage wipe；
- pre-Cancun `SELFDESTRUCT` 的 slot preimage；
- EVM 交易执行；调用方直接提供每个 block 的账户变更；
- MDBX、RocksDB、Static Files 或数据库事务引擎；全部用内存容器模拟；
- Tokio、真实线程池、Rayon 并行和真实 channel；本示例保留等价的单线程事件队列和模拟 proof worker，以展示调度语义；
- 自研 MPT 节点编码、hex-prefix、RLP、根哈希计算和 sparse trie 内核；这些全部复用 `alloy-trie` 与 `reth-trie-sparse`；
- fork 特有的 Proof V2 `min_len` 后缀证明语义；本示例采用 upstream 的整条路径 multiproof + 目标级去重；
- trie node 的生产级压缩格式、磁盘 codec 和性能优化；
- fork choice、多条并行 fork 和长期非 canonical block 保存；
- archive node 的完整持久化生命周期。

账户 trie 和 storage trie 在“hashed key、proof、局部 reveal、dirty ancestor 重算”方面同构，但并非完全等价。storage 还有嵌套 root、整组 wipe 和 preimage 等额外问题，本示例不覆盖这些差异。

## 3. 复用策略与真实 Reth 的对应关系

### 3.1 复用清单

| 来源 | 直接复用 | 用途 |
|---|---|---|
| `alloy-trie` | `Nibbles`、`TrieNode`、RLP 编解码、hex-prefix、`EMPTY_ROOT_HASH` | 全部编码与密码学管道 |
| `alloy-trie` | `HashBuilder` | full-rebuild oracle：全量叶子有序流重算 root |
| `alloy-trie` | `HashBuilder` + `ProofRetainer` | proof 生成：从有序叶子流收集目标路径节点，公共节点天然去重 |
| `alloy-trie` | `proof::verify_proof` | 测试中校验/篡改 proof |
| `reth-trie-sparse` | `SerialSparseTrie`（实现 `SparseTrieInterface`） | sparse trie 内核：blind/reveal/update/remove/root/updates |

自研的只有三块：**调度器**（事件循环、sequencer、去重、终止条件）、**存储模拟**（`StateDb` 各表、changeset、history）和 **driver**（`BlockInput` 展开为事件、root 校验、原子 commit）。

作为使用方的集成面刻意保持最小：

- 实现一个单方法 trait：`TrieNodeProvider::trie_node(path) -> Result<Option<RevealedNode>>`；第一版可用库自带的 `DefaultTrieNodeProvider`（永远返回 `None`，即「无兜底」）占位；
- 调用 `SerialSparseTrie` 的六个方法：`with_root` / `reveal_nodes` / `update_leaf` / `remove_leaf` / `root` / `take_updates`；接口上其余方法不使用即等于不存在；
- 不写 facade、不写适配层；出现第二个实现前不做抽象。

版本对齐：`alloy-trie` 版本必须与 `reth-trie-sparse` 所属 reth workspace 咬合，建议以 git dependency 指向 reth 仓库同一 tag。若日后需要 trie 内核并行，`ParallelSparseTrie`（reth-trie-sparse-parallel）实现同一 trait，可零重构替换；本示例不使用——内核内部并行与调度系统正交。

### 3.2 概念对应表

| 本示例 | Reth 2.0 中的概念 | 说明 |
|---|---|---|
| `HashedAccounts` | MDBX `HashedAccounts` | canonical latest account state |
| `TrieDb` | MDBX `AccountsTrie` + hashed account cursor | proof 和持久化 trie 节点的数据源 |
| `HashedPostState` | `HashedPostState` | 当前 block 的 hashed account diff |
| `SerialSparseTrie`（直接复用） | upstream `reth-trie-sparse` 同一类型 | 生产引擎的 account trie 用并行实现（`ParallelSparseTrie`），接口相同 |
| `SparseTrieCache` | `PreservedSparseTrie` + task 的 reuse/prune 路径 | 跨 block 复用已经 reveal 的节点 |
| `TrieScheduler` | upstream `MultiProofTask` + `SparseTrieTask` 的单线程合并 | 唯一状态 owner；派生 targets、去重、派发、sequencer 排序、reveal、apply、判断 drain |
| `SimulatedProofWorker` | account proof worker pool | 用 `HashBuilder` + `ProofRetainer` 同步执行，再以 completion event 返回；不直接修改 sparse trie |
| `ProofTargets`（hashed key 集合） | upstream `MultiProofTargets` | 目标级去重；不采用 fork 的 `min_len` 后缀语义 |
| `TrieUpdates` | account trie updates | dirty nodes 的 upsert/delete 集合 |
| `AccountChangeSets` | account changeset static files | 本示例用按 block 排列的内存 vector |
| `AccountHistory` | RocksDB account history index | 本示例用 `address -> sorted blocks` |

本示例追求机制一致，不追求 Reth 数据库的字节级兼容。特别是 `TrieDb` 可以先保存完整的 encoded trie nodes，以降低 proof builder 的实现难度；`HashedAccounts` 仍然是账户值的 canonical source of truth。后续若要进一步贴近 Reth，可以把 `TrieDb` 换成类似 `BranchNodeCompact` 的内部节点表，而不改变 sparse trie、cache 和 historical API。

## 4. 术语与约定

### 4.1 状态时刻

本文统一定义：

- `state(N)`：执行完 block `N` 后的状态；
- block `N` 的 changeset 保存执行该 block 前的值，也就是从 `state(N)` 回退到 `state(N-1)` 所需的数据；
- genesis 记作 block `0`；
- `head` 总是当前 canonical block number。

该约定必须贯穿 latest query、historical query 和 unwind，禁止混用“block 开始状态”与“block 结束状态”。

### 4.2 Key

- `Address`：20 字节原始账户地址；
- `HashedAddress = keccak256(Address)`：32 字节；
- `TriePath`：`HashedAddress` 展开后的 64 个 nibble；
- trie 中所有顺序和 prefix 操作都基于 `TriePath`，不是 plain address。

### 4.3 Account

账户至少包含：

- `nonce`；
- `balance`；
- `code_hash`；
- `storage_root`，在本示例中固定为 empty trie root。

账户 leaf value 按 Ethereum 账户顺序编码为 `[nonce, balance, storage_root, code_hash]` 的 RLP。相同账户必须总是得到相同 bytes；state root 计算不得依赖内存地址、map 迭代顺序或调试字符串。

空账户是否删除必须由调用方明确表达。本示例不自行实现 EIP-161 判断，而使用 `Option<Account>`：

- `Some(account)`：插入或更新；
- `None`：删除账户。

## 5. Ethereum MPT 最小模型

账户 trie 是十六叉 Merkle Patricia Trie。节点语义包括：

- `Empty`：空节点；
- `Leaf`：剩余 nibble path 和 account RLP；
- `Extension`：公共 path prefix 和单个 child reference；
- `Branch`：最多 16 个 child references。本示例的 key 固定为 64 nibbles，因此 branch value 恒为空。

节点编码和引用规则遵循 Ethereum MPT（hex-prefix compact encoding、RLP、小于 32 字节 inline reference、标准 root hash、empty trie root），但**全部由 `alloy-trie` 提供，本项目一行不实现**。

reference implementation 同样是白拿的：把全量 `HashedAccounts` 按 hashed key 排序后喂给 `HashBuilder` 重算 root，约二十行，作为 sparse update 结果的对拍 oracle。由于编码管道现成，v0.2 中「所有节点都 hash 的 toy 模式」降级路径不再需要，删除。

## 6. 内存数据库布局

所有表都属于一个逻辑数据库，commit 时必须表现为原子更新。

### 6.1 `HashedAccounts`

```text
HashedAddress -> Account
```

性质：

- 这是 latest state 的唯一 canonical account table；
- 不存在 `Address -> Account` 的 plain-state 副本；
- latest lookup 接收 plain address，在 provider 边界计算 hash；
- 表必须按 `HashedAddress` 有序，便于 deterministic trie construction 和 proof generation。

### 6.2 `TrieDb`

```text
NibblePath -> PersistedTrieNode
CurrentRoot -> StateRoot
```

第一版允许保存完整 encoded MPT nodes。它是由 `HashedAccounts` 派生出的 Merkle index，不是账户状态的第二个 canonical source。proof 生成不依赖节点表：直接用 `HashBuilder` + `ProofRetainer` 遍历有序 `HashedAccounts` 构造，multiproof 的公共节点去重由 `ProofRetainer` 天然保证；节点表主要用于演示 `TrieUpdates` 的持久化推进。

必须提供：

- 根据 path 读取持久化节点；
- 在某个已提交 root 上生成 inclusion/exclusion multiproof；
- 应用 `TrieUpdates` 中的 node upserts 和 deletes；
- 返回当前已提交 root；
- 在测试中从完整 `HashedAccounts` 重建并核对 root。

任何时候都必须满足：

```text
TrieDb.root == full_trie_root(HashedAccounts)
```

### 6.3 `AccountChangeSets`

```text
BlockNumber -> ordered list of (Address, AccountBefore)
AccountBefore = Some(Account) | None
```

要求：

- 使用 plain `Address`，不是 `HashedAddress`；
- 每个账户在同一个 block 中最多出现一次；
- 保存 block 执行前的值；
- `None` 表示账户在 block 前不存在；
- 同时服务 historical query 和 unwind。

### 6.4 `AccountHistory`

```text
Address -> strictly increasing list of BlockNumber
```

列表记录该账户发生 canonical change 的 block。它模拟 Reth 放在 RocksDB 中的历史索引。

### 6.5 `BlockMeta`

```text
BlockNumber -> {
    parent_root,
    state_root
}
```

至少保存每个未被裁剪 block 的 parent root 和 resulting root，用于验证 commit 和 unwind。

## 7. Block 输入与派生数据

调用方提交一个 block 时提供：

```text
BlockInput {
    number,
    parent_root,
    expected_root: Option<StateRoot>,
    account_changes: Address -> Option<Account>
}
```

输入要求：

- `number == head + 1`；
- `parent_root == BlockMeta[head].state_root`；
- `expected_root` 若存在，模拟 block header 的 state root，prepare 结果必须与之相等；
- 同一个地址只能有一个最终变更；
- no-op update 可以在预处理阶段删除。

由输入派生：

```text
HashedPostState:
    keccak256(address) -> encoded new account | Delete

AccountChangeSet:
    address -> HashedAccounts[keccak256(address)] before commit
```

changeset 必须在修改 `HashedAccounts` 之前读取。账户 hash、变更排序和 changeset 构造虽然可以实现为不同步骤，但逻辑上属于同一个 block transaction。

## 8. 事件循环与调度模型

本示例即使不使用异步 runtime，也必须显式实现事件循环。目的不是模拟线程，而是把 Reth 的核心控制逻辑展示出来：一个 owner 独占 working state，外部输入和 worker completion 只通过事件改变它。

### 8.1 六元组

调度器定义为：

```text
TrieScheduler = (State, Events, Transitions, Readiness, Policy, Commit)
```

| 维度 | 本示例中的定义 |
|---|---|
| `State` | block generation、working sparse trie、fetched targets、inflight work、sequencer 缓冲、finish 标记 |
| `Events` | begin block、account update、prefetch、finish、proof completed、cancel |
| `Transitions` | hash、派生 targets、去重、dispatch、sequencer 放行、reveal、apply（provider 兜底）、root、prepare、commit |
| `Readiness` | proof 依赖是否满足、worker capacity 是否可用、输入是否已结束、所有 work 是否 drain |
| `Policy` | 事件优先级、proof target 去重、chunk size、completion coalescing、prefetch |
| `Commit` | sparse root、full-rebuild oracle 和 expected root 一致后原子 publish |

正确性规则与调度策略必须分开：

```text
Correctness:
    blind 不能当作不存在
    stale proof 不能 reveal
    state 变更必须按 sequence number 顺序应用
    root 不一致不能 commit

Policy:
    chunk_size 取多少
    先处理 update 还是 completion
    一次 coalesce 多少 proof results
    cache 保留多少热节点
```

改变 policy 可以改变执行顺序和性能，但不能改变最终 root、TrieUpdates 或 historical state。

### 8.2 唯一状态 owner

`TrieScheduler` 是以下 mutable state 的唯一 owner：

```text
SchedulerState {
    generation,
    phase,
    parent_root,
    working_trie,
    final_hashed_state,
    fetched_targets,
    next_sequence,
    inflight_proofs,
    sequencer,          // 乱序 completion 的重排序缓冲
    finished_updates,
    prepared_block,
}
```

`SimulatedProofWorker` 只能：

```text
ProofRequest -> ProofResult
```

它不得直接 reveal 或修改 `working_trie`。这样即使以后把同步 worker 换成线程池，canonical working state 仍只有一个写入者。

调度器 phase 只允许以下转换：

```text
Idle
  -> Collecting
  -> ResolvingDependencies
  -> RootReady
  -> Prepared
  -> Committed

任意未提交 phase
  -> Failed / Cancelled
  -> Idle（保留旧 committed DB/cache）
```

`FinishUpdates` 只触发 `Collecting -> ResolvingDependencies`；它不能跳过 proof drain 直接进入 `RootReady`。

### 8.3 事件字母表

最小事件集合：

```text
BeginBlock {
    generation,
    number,
    parent_root,
    expected_root?,
}

PrefetchAccount {
    generation,
    address,
}

AccountUpdate {
    generation,
    address,
    new_value: Option<Account>,
}

FinishUpdates { generation }

ProofCompleted {
    generation,
    sequence,
    result,
}

CancelBlock { generation }
```

`generation` 标识这批异步工作依赖的 parent state。即使第一版同步执行，也必须保留该字段并测试 stale completion：reorg/cancel 后到达的旧 proof result 应直接丢弃，不能 reveal 到新 root 的 cache。`sequence` 是派发时领取的序号，sequencer 依赖它做连续段放行。

为了保持输入简单，`BlockInput.account_changes` 仍规定同一地址只有一个最终值。driver 负责将 `BlockInput` 展开为：

```text
BeginBlock
AccountUpdate*
FinishUpdates
```

prefetch event 是可选的；它只生成 touched/read-only target，不得改变最终 hashed state。

### 8.4 业务对象的状态流转

一个 account update 的生命周期是：

```text
Received
  -> Hashed
  -> TargetsDerived        // 与 fetched_targets 求差
  -> ProofInFlight         // 差集为空时走 EmptyProof 短路，跳过此态
  -> Sequenced             // 在 sequencer 中等待连续段
  -> Revealed + Applied    // reveal multiproof 后按序 apply；漏网 blind 由 provider 兜底
  -> RootComputed
  -> Validated
  -> Committed
```

预先 hash 的输入可以跳过 `Hashed`；目标全部取过的 update 走 EmptyProof 短路，跳过 `ProofInFlight`（但仍占 sequence number、仍经 sequencer）。

集合之间的转换为：

```text
state update
    │ hash + 与 fetched_targets 求差
    ├── 差集为空 ──> EmptyProof(seq) ────────────┐
    │ 差集非空：领 seq、登记 fetched_targets      │
    ▼                                           ▼
inflight_proofs ── ProofCompleted(seq) ──> sequencer
                                             │ 连续段就绪
                                             ▼
                              reveal multiproof + 按序 apply
                                             │ 撞到漏网 blind
                                             └─> provider 同步兜底（miss = 硬错误）
```

与 fork 风格不同，这里没有 fault 上报和重试循环：`SerialSparseTrie::update_leaf` 撞到 blinded 节点时通过传入的 `TrieNodeProvider` 同步取数，对调度器透明。调度器要守住的是另一件事：**同一账户被多个 sequence 触及时，应用顺序必须等于 sequence 顺序**（后面的值覆盖前面的），这由 sequencer 只放行连续段保证；而 reveal 是幂等可交换的，proof 计算本身可以乱序并行。

### 8.5 Readiness 规则

每类工作何时 ready 必须显式定义：

| 工作 | Ready 条件 |
|---|---|
| dispatch proof | 目标与 `fetched_targets` 的差集非空，且 inflight 数小于 `max_inflight_proofs` |
| EmptyProof 短路 | 目标已全部出现在 `fetched_targets` |
| reveal + apply | completion generation 等于 current generation，且 sequence 与已放行前缀连续 |
| compute root | `FinishUpdates` 已收到，派发数等于回收数，sequencer 为空 |
| prepare commit | root 已计算，TrieUpdates 和 changeset 已生成，full-rebuild oracle 通过 |
| atomic commit | expected root（若提供）匹配，cache anchor 仍等于 parent root |

核心原则是：

> 被阻塞的是某个业务 update，不是 scheduler 线程。只要其他 update 已 ready，事件循环仍可推进它们。

### 8.6 调度策略

为贴近当前 Reth，确定性模拟器采用以下 policy：

1. 优先接收已经排队的 account update/prefetch/finish events；
2. 收到一个 proof completion 后，coalesce 当前已经完成的其他 proof results，再一次性 reveal；
3. 处理所有因 reveal 而新 ready 的 pending updates；
4. pending targets 超过 `chunk_size`，或输入队列暂时为空时，派发 proof requests；
5. `target_count >= 2 * chunk_size` 且存在多个空闲 worker capacity 时允许 chunk；
6. target 数超过 `force_chunk_threshold` 时强制 chunk，防止一个大任务长期占用 worker；
7. 完全空闲时可以预计算 clean subtrie hashes；第一版可省略该优化。

建议默认配置：

```text
chunk_size = 5
force_chunk_threshold = 300
max_inflight_proofs = 2
```

这里的 5 和 300用于演示当前 Reth 的调度语义，不是共识参数。测试必须证明改变它们只改变 dispatch 次数和 completion 顺序，不改变结果。

### 8.7 单线程 worker 模拟

不需要为教学示例引入 Tokio。driver 持有两个队列：

```text
event_queue: VecDeque<Event>
work_queue:  VecDeque<ProofRequest>
```

每轮可以执行：

```text
1. scheduler.step(event) 产生零个或多个 ProofRequest
2. driver 从 work_queue 选择一个请求并同步计算 proof
3. driver 把 ProofCompleted 放回 event_queue
4. 可在测试中故意改变 completion 顺序
```

这样既保留了 command/completion 的异步边界，也能让测试完全确定、没有 sleep 和竞态。后续替换为真实 worker pool 时，scheduler 的状态机和事件接口不变。

### 8.8 主循环骨架

逻辑主循环为：

```text
while !scheduler.terminal():
    event = next_event_or_worker_completion()
    scheduler.handle_event(event)

    scheduler.promote_ready_updates()
    scheduler.verify_and_reveal_completions()

    while scheduler.has_capacity():
        if let Some(work) = scheduler.choose_ready_proof_work():
            dispatch(work)
        else:
            break

    if scheduler.ready_to_compute_root():
        candidate = scheduler.compute_root_and_prepare()

    if scheduler.ready_to_commit(candidate):
        db.atomic_commit(candidate)
        scheduler.publish_cache(candidate)
```

实际代码可以将这些步骤拆成纯函数。主循环只 orchestration，不应同时塞入 proof 构造、MPT 编码和数据库写入细节。

### 8.9 Drain、commit 与失败

`FinishUpdates` 不是终止条件，只是关闭输入的一部分。成功 drain 必须满足：

```text
finished_updates
&& dispatched_count == completed_count
&& sequencer.is_empty()
```

只有 drain 后才能计算最终 root。root 计算成功也不是 commit：必须再通过 expected root/full-rebuild oracle，最后 clone-and-swap 数据库和 cache。

失败路径：

- invalid proof：block 失败，working trie 丢弃，DB/cache anchor 不变；
- stale completion：静默丢弃并增加 metric；
- wrong parent root：拒绝 `BeginBlock`；
- cancel/reorg：generation 前进，清空 inflight/sequencer bookkeeping，以新 parent root 冷启动；
- apply 阶段 provider miss：硬错误，block 失败。

在 upstream 合同下，v0.2 的 no-progress/deadlock 检查退化为最后这条硬规则：不存在可自旋的重试循环，proof 覆盖不足会在 apply 时立即以 provider miss 暴露，说明 target 派生或 proof 生成有 bug。


## 9. Sparse trie 表示

Sparse trie 不需要展开整棵 trie。未展开子树只保存一个承诺 hash。**本节是行为语义与库概念的对照，不是实现规格——`SerialSparseTrie` 已实现全部语义。**

### 9.1 Sparse node（映射到 `reth-trie-sparse` 的 `SparseNode`）

| 逻辑状态 | 库中对应 |
|---|---|
| `Empty`（已知为空） | `SparseNode::Empty` |
| `RevealedLeaf` / `RevealedExtension` / `RevealedBranch` | `SparseNode::Leaf` / `Extension` / `Branch` |
| `Blinded(hash)`（只知子树 hash） | `SparseNode::Hash` |
| `Dirty` / `Clean`（cached hash 失效/有效） | 节点内 `hash: Option<B256>` 的 `None` / `Some` |

`Blinded(hash)` 不是“数据不存在”，而是“数据存在性及内容未知，但其 Merkle commitment 已知”。因此 lookup 返回 `None` 不能同时表达“不存在”和“尚未 reveal”——库的 `find_leaf` 已区分这三种结果：`LeafLookup::Exists`、`LeafLookup::NonExistent`、`Err(LeafLookupError::BlindedNode { path, hash })`。

### 9.2 Cache anchor

Sparse trie cache 必须绑定到一个明确的 canonical state：

```text
CacheAnchor {
    block_number,
    state_root
}
```

只有当 block input 的 `parent_root` 等于 cache anchor root 时才能复用 cache。否则必须 invalidate，并从 `parent_root` 建立一个只有 `Blinded(parent_root)` 的冷 cache。

空 trie 是例外：它可以直接以 revealed `Empty` 初始化。

## 10. Proof 与 partial proof

### 10.1 Proof 的职责

proof 必须让 sparse trie 在不读取整棵树的情况下确定：

- 目标账户存在及其旧 leaf value；或
- 目标账户不存在；
- 从目标路径到某个已知 ancestor/subtree commitment 的所有必要结构；
- 更新后重算 ancestor hash 所需的 sibling references。

proof 必须以 `parent_root` 对应的已提交 TrieDB 为数据源，不能从正在修改的 sparse trie 或未提交 `HashedPostState` 生成。

### 10.2 Proof target

采用 upstream 的目标级语义：target 就是 hashed account key 的集合（对应 `MultiProofTargets` 的 account 部分）。

- 从 state update 派生：本批触及的所有 hashed keys 与 `fetched_targets` 求差；
- 差集为空时不派发 proof，走 EmptyProof 短路；
- 不采用 fork 的 `(key, min_len)` 后缀语义。「不重复取」的粒度是整个 target，而非节点后缀：首次取回的 proof 可能包含 sparse trie 已 reveal 的浅层节点，重复 reveal 是幂等无害的。

### 10.3 Multiproof 与去重

一次 dispatch 为一批 targets 生成一个多重证明（multiproof）：用 `HashBuilder` + `ProofRetainer` 遍历 parent state 的有序叶子，多个 key 共享的路径节点只返回一次（`ProofRetainer` 天然去重）。

reveal 前必须验证：

1. proof 节点能重建出 parent root，或与 sparse trie 中对应 blinded boundary 的 hash 一致；
2. inclusion proof 的 leaf value 与 parent-state `HashedAccounts` 一致；
3. exclusion proof 确实在 empty child 或不同 leaf 处终止；
4. proof 不得与 cache 中已 reveal 的节点内容冲突。

如果验证失败，整个 block 失败，cache 和数据库都不得前进。（本项目的 proof 由自己的 TrieDb/flat store 生成，验证逻辑主要服务于篡改测试场景 H。）

### 10.4 删除时的 blinded sibling

删除 leaf 可能让 branch 只剩一个 child 触发 collapse；若剩余 sibling 是 blinded，必须先 reveal 其结构才能决定压缩后的 extension/leaf path。

在 upstream 合同下这对调度器完全透明：`SerialSparseTrie::remove_leaf` 内部通过传入的 `TrieNodeProvider` 同步取回 sibling 节点。本项目只需保证 provider 能按 path 出具单节点 proof（同样用 `HashBuilder` + `ProofRetainer` 实现）。不存在 fork 风格「collapse 产生新 target、多轮 fetch/retry」的循环。

## 11. Sparse trie update 算法

插入/更新/删除、Patricia normalization、dirty 重算和 root 计算**全部由 `SerialSparseTrie` 提供，本项目不实现**。调用方合同收缩为：

1. 输入按 `HashedAddress` 排序（保证 deterministic 输出顺序；map 迭代顺序不得影响结果）；
2. `Set(encoded_account)` 调 `update_leaf`，`Delete` 调 `remove_leaf`；同一批内删除延后到全部插入/更新之后（沿用 upstream 的 deferred removals：避免中间分支先删后建）；
3. 两个调用都传入 `TrieNodeProvider`；apply 阶段 provider miss 视为硬错误；
4. 全部应用后调 `root()` 取新根、`take_updates()` 取 `TrieUpdates`。

行为要求保持不变并由测试守住：批量更新与逐个更新得到相同 root；删除不存在账户是 no-op，但必须已有 exclusion knowledge，不能把 blind 当作不存在。

## 12. TrieUpdates

`TrieUpdates` 描述如何把持久化 TrieDB 从 parent root 推进到 new root：

```text
TrieUpdates {
    upserts: NibblePath -> PersistedTrieNode,
    deletes: set of NibblePath,
    new_root
}
```

要求：

- 只包含实际变化的节点；
- 被 Patricia normalization 消除的旧 path 必须进入 deletes；
- 同一路径不能同时保留矛盾的最终 upsert/delete；
- 应用 updates 后，从 TrieDB 生成的 proof 必须对应 `new_root`；
- `new_root` 必须等于 sparse trie root，也必须等于 full rebuild root。

`TrieUpdates` 直接采用 `take_updates()` 返回的 `SparseTrieUpdates`（`BranchNodeCompact` 的 upserts 与 removed paths），不再自定义格式。`TrieDb` 的持久化节点表相应保存 compact branch nodes 或完整 encoded nodes 均可，外部 commit 协议不变。

## 13. Sparse trie cache 生命周期

### 13.1 Cold start

```text
cache.anchor = current head/root
cache.trie   = Blinded(current root)
```

第一个 block 对所有 changed accounts 都需要 proof。

### 13.2 Warm update

block commit 成功后：

- sparse trie 保留当前已经 reveal 的节点；
- dirty nodes 变为 clean，并缓存最新 RLP/hash；
- anchor 更新为新 block/new root；
- proof target 去重集合和本轮 pending updates 清空；
- 未涉及的 blinded subtrees 保持 blinded。

后续 block 如果再次修改已 reveal 路径，应该直接 cache hit；只有走入未 reveal boundary 时才请求 partial proof。

### 13.3 Cache pruning

这里的 pruning 是内存 cache pruning，不是 historical pruning。注意 `SerialSparseTrie` **没有**「把冷子树折叠回 blinded」的 API（fork 的 arena 实现才有），因此最小实现二选一：

- 不做容量 pruning，cache 在示例运行期内自然增长（教学示例可接受）；
- 超过 `max_cached_nodes` 时丢弃整棵 cache，下一 block 以新 root 冷启动（等价于粗粒度全量 prune）。

无论哪种，都必须保持：root 不受影响、dirty 状态不跨 commit 存活、cache 的存在与否只影响 proof I/O 量而不影响任何结果。

### 13.4 Reorg

最小实现不尝试反向修改 sparse cache：

1. 使用 changesets 和反向 trie updates unwind 数据库；
2. 核对恢复后的 root；
3. 丢弃整个 cache；
4. 以恢复后的 root 建立新的 cold cache。

这牺牲 reorg 后的 cache 命中率，但状态机简单且正确。缓存优化不能成为 consensus correctness 的前提。

## 14. Commit 协议

一个 block 的处理分为 prepare 和 commit 两阶段。

### 14.1 Prepare

Prepare 不修改持久化内存 DB：

1. 校验 block number 和 parent root。
2. 从 `HashedAccounts` 读取 before values，构造 plain-address changeset。
3. 将 address changes hash 成 `HashedPostState` 并排序。
4. 派生 proof targets、去重后派发 multiproof，结果经 sequencer 按序 verify + reveal 到 cache 的 working copy。
5. 按序 apply leaf updates；漏网 blinded 节点由 provider 同步兜底。
6. 计算 `new_root` 和 `TrieUpdates`。
7. 在 `HashedAccounts` 的临时副本上应用 `HashedPostState`，再用 full rebuild oracle 校验 root；测试构建中必须执行，普通运行可配置关闭。

### 14.2 Atomic commit

只有 prepare 全部成功后才一次性应用：

1. 写 `AccountChangeSets[block]`；
2. 把 block number append 到相关 `AccountHistory[address]`；
3. 应用 `HashedPostState` 到 `HashedAccounts`；
4. 应用 `TrieUpdates` 到 `TrieDb`；
5. 写 `BlockMeta` 并更新 head；
6. 将 working sparse trie 发布为新 cache。

任一步失败都必须保持以下内容不变：

- head；
- HashedAccounts；
- TrieDB root；
- history/changesets；
- cache anchor。

内存模拟可以通过 clone-and-swap 实现原子性，不需要实现真实事务日志。

## 15. Latest account 查询

接口接收 plain address：

```text
latest_account(address):
    hashed = keccak256(address)
    return HashedAccounts[hashed]
```

不存在 plain account table，也不需要 hash preimage table，因为调用方本来就提供 address。

## 16. Historical account 查询

目标是查询 `state(target_block)` 中的账户值。

算法：

1. 在 `AccountHistory[address]` 中寻找严格大于 `target_block` 的最小变更 block `B`。
2. 如果找到 `B`，返回 `AccountChangeSets[B][address].AccountBefore`。
3. 如果找不到，说明 target block 之后该账户没有再变化，返回 latest：
   `HashedAccounts[keccak256(address)]`。

示例：

```text
block 1: Alice 不存在 -> balance 10
block 4: balance 10 -> balance 20
block 7: balance 20 -> 删除

history[Alice] = [1, 4, 7]
changeset[1]   = None
changeset[4]   = balance 10
changeset[7]   = balance 20

account_at(0) = changeset[1] = None
account_at(1) = changeset[4] = balance 10
account_at(5) = changeset[7] = balance 20
account_at(7) = latest       = None
```

边界要求：

- `target_block > head` 返回错误；
- 请求已被 history pruning 覆盖的 block 返回 `HistoryUnavailable`，不能错误回退到 latest；
- history index 与 changeset 缺失或不一致视为数据库损坏。

## 17. Historical pruning

historical pruning 与 hashed-state table 的 key 顺序无关。当前状态始终保留；被裁剪的是按 block 组织的 changesets 和对应 history entries。

定义 `history_floor`：系统保证可以查询的最早 `state(N)`。

把 floor 从 `F_old` 推进到 `F_new` 时：

1. 删除 block number `<= F_new` 的 changesets；
2. 从每个 `AccountHistory[address]` 删除 `<= F_new` 的 entries；
3. 更新 `history_floor = F_new`；
4. 保留所有 `> F_new` 的变更，因为查询 `state(F_new)` 时仍可能需要下一个变更 block 的 before value。

注意：如果仍需要 unwind 到某个 block，则从当前 head 回退到该 block 所需的 changesets 不能被 prune。实现应单独定义 unwind retention window，并禁止 historical pruning 越过仍受支持的最早 unwind 目标。

## 18. Unwind

从 head `H` unwind 一个 block：

1. 读取 `AccountChangeSets[H]`；
2. 对每个地址计算 `keccak256(address)`；
3. `AccountBefore = Some(account)` 时恢复 `HashedAccounts`；
4. `AccountBefore = None` 时从 `HashedAccounts` 删除；
5. 从 `AccountHistory[address]` 移除尾部的 `H`；
6. 恢复 TrieDB 到 `BlockMeta[H].parent_root`；
7. 删除 block `H` 的 changeset/meta 并令 `head = H - 1`；
8. 丢弃 sparse trie cache，以恢复后的 root 冷启动。

为了保持代码最小，TrieDB unwind 可以采用以下任一种策略：

- 每个 block 同时保存反向 `TrieUpdates`；这是推荐方案；
- 根据恢复后的完整 `HashedAccounts` 全量重建 TrieDB；更简单但无法展示真实增量 unwind。

无论采用哪一种，unwind 后必须验证：

```text
TrieDb.root
    == BlockMeta[head].state_root
    == full_trie_root(HashedAccounts)
```

## 19. 组件边界

建议保持以下逻辑组件，即便最后每个组件只有少量代码：

### `StateDb`

拥有 HashedAccounts、TrieDB、changesets、history、block meta 和 head；提供原子 commit/unwind。

### `TrieDb`

拥有持久化 trie 视图；负责 proof generation、应用 trie updates 和 full rebuild oracle。

### `SparseTrie`（复用，不实现）

即 `reth_trie_sparse::SerialSparseTrie`。只理解 hashed path、revealed/blinded nodes、leaf updates、proof reveal、root 和 trie updates；不知道 address、block 和 history。本项目只调用其六个方法，不包 facade。

### `FlatStoreProvider`

实现 `TrieNodeProvider` 单方法 trait：按 path 从 parent-state 数据出具单节点证明，作为 apply 阶段撞到漏网 blinded 节点时的同步兜底。第一版可用库自带的 `DefaultTrieNodeProvider`（永远 miss）占位，此时调度器必须保证 multiproof 预取全覆盖。

### `SparseTrieCache`

拥有跨 block 的 cache anchor 和 committed SparseTrie；负责 cache reuse、invalidation 与可选内存 pruning。它不持有某一 block 尚未验证的 canonical working state。

### `TrieScheduler`

当前 block working trie 和调度集合的唯一 owner；负责事件转换、readiness、proof target 去重、inflight bookkeeping、completion 排序（sequencer）、reveal、apply、drain 与 commit candidate 生成。

### `SimulatedProofWorker`

用 `HashBuilder` + `ProofRetainer` 从 parent-state 有序叶子同步构造 multiproof，执行 `ProofRequest -> ProofResult`。它不读取未提交 post-state，也不直接修改 scheduler/cache——结果必须包装成 completion 事件交回调度器。

### `BlockProcessor`

作为 driver 将 `BlockInput` 展开为事件，驱动 scheduler 与 simulated workers，连接 changeset、state-root validation 和 atomic commit。它不复制 scheduler 的依赖状态。

### `HistoricalProvider`

只通过 history index、changesets 和 latest hashed state 实现 `account_at`；不依赖 sparse trie cache。

这个边界很重要：cache 丢失、重启或 pruning 只能影响性能，不能影响 latest/historical state 的正确性。

## 20. 必须保持的 invariants

每次成功 commit/unwind 后检查：

1. `TrieDb.root == full_trie_root(HashedAccounts)`。
2. `BlockMeta[head].state_root == TrieDb.root`。
3. `cache.anchor.root == TrieDb.root`；cache 被显式丢弃的瞬间除外。
4. cache 中每个 blinded hash 都是 anchor state 下对应 subtree 的 commitment。
5. cache reveal/prune 前后 root 不变。
6. 同一输入无论 cold cache、warm cache 或 cache pruning，得到的 root 相同。
7. changeset 使用 plain address，latest state 使用 hashed address。
8. `AccountHistory[address]` 严格递增，并与 changesets 一一对应。
9. historical query 不依赖 Keccak preimage。
10. invalid proof、错误 parent root 或 prepare 失败不会产生部分 commit。
11. working sparse trie 只有 `TrieScheduler` 一个 mutable owner；worker completion 只能通过事件应用。
12. 任一 `ProofCompleted.generation` 必须匹配 current generation，否则不得 reveal。
13. 收到 `FinishUpdates` 后仍必须等派发的 proof 全部回收、sequencer 清空，不能提前求根。
14. 改变事件到达顺序、proof completion 顺序、`chunk_size` 或 worker capacity，不改变最终 root 和 DB 内容。
15. apply 阶段 provider miss 视为硬错误立即失败；调度器中不存在可自旋的重试循环。

## 21. 验收场景

### A. Genesis

- 初始化多个具有公共 nibble prefix 的账户；
- full rebuild 得到 genesis root；
- TrieDB 和 BlockMeta root 一致；
- cache 以 genesis root blinded 启动。

### B. Cold-cache account update

- 修改一个现有账户；
- 调度器为其派生 target、派发 multiproof、经 sequencer 放行并 reveal；
- apply 全程不触发 provider 兜底（proof 覆盖完整）；
- sparse root 等于 full rebuild root。

### C. Warm-cache update

- 下一 block 再修改同一账户；
- 不需要完整 root-to-leaf proof；
- proof node 数少于 cold run，理想情况下为零。

### D. 已取目标短路（EmptyProof）

- 同一 block 内两批 state update 触及同一账户；
- 第二批与 `fetched_targets` 求差后为空，不派发 proof，走 EmptyProof；
- EmptyProof 仍占 sequence number 并经 sequencer 按序应用；
- root 正确，该账户的 proof 派发计数为 1。

### E. Multiproof 去重

- 一个 block 同时修改多个共享 prefix 的账户；
- 公共 proof nodes 只返回一次；
- 变更输入顺序不影响 root。

### F. Non-existence insertion

- 向一个不存在路径插入账户；
- exclusion proof 在 empty child 或不同 leaf 处结束；
- leaf split/branch creation 正确。

### G. Delete and branch collapse

- 删除一个使 branch 只剩单 child 的账户；
- 若 sibling blinded，`remove_leaf` 通过 provider 同步取回 sibling（断言 provider 被调用）；
- normalization 后 extension/leaf 正确；
- obsolete trie paths 出现在 `TrieUpdates` 的 removed paths。

### H. Invalid proof

- 篡改 sibling hash、leaf value 或 path；
- reveal 被拒绝；
- DB head/root/cache anchor 均不改变。

### I. Historical account

- 覆盖不存在、创建、连续更新、删除四种状态；
- 对每个 block 查询都返回 block 结束后的正确值；
- latest fallback 只发生在 target 后没有变更时。

### J. Reorg/unwind

- 连续提交至少三个 block；
- unwind 一个和多个 block；
- hashed state、history、TrieDB root 全部恢复；
- cache 被安全冷启动。

### K. History pruning

- 推进 `history_floor`；
- floor 之前查询返回 `HistoryUnavailable`；
- floor 及之后查询保持正确；
- latest state 和 state root 完全不受影响。

### L. Cache 丢弃与冷启动

- 多个 block 后主动丢弃整棵 cache（模拟容量上限触发的粗粒度 prune）；
- 下一 block 冷启动，root 与 warm 路径完全一致；
- proof 派发数与 reveal 节点数显著高于 warm block，证明 cache 只影响 proof I/O，不影响结果。

### M. Out-of-order proof completion

- 同时产生至少两个 proof chunks；
- 分别按派发顺序和逆序返回 completion；
- 两次运行的 root、TrieUpdates 和 committed DB 完全相同。

### N. Stale generation

- generation 1 派发 proof 后 cancel/reorg；
- generation 2 从另一个 parent root 开始；
- 此时注入 generation 1 的 completion；
- scheduler 丢弃旧结果，generation 2 cache 不发生变化。

### O. Finish is not drain

- 在 proof inflight 时发送 `FinishUpdates`；
- scheduler 不得立即计算 root；
- 所有 completion 经 sequencer 放行、reveal + apply 完成后才进入 `RootComputed`。

### P. Backpressure and chunking

- 设置不同 `chunk_size`、`max_inflight_proofs` 和 completion 顺序；
- inflight 数始终不超过限制；
- dispatch 数量可以变化，但 root 和 commit 内容不变。

### Q. Provider miss 硬错误

- 构造覆盖不完整的 multiproof，且 provider 使用 `DefaultTrieNodeProvider`（永远 miss）；
- apply 撞到漏网 blinded 节点时立即返回错误；
- block 失败，不进入无限循环，不修改 DB/cache anchor。

## 22. 可观测性

为了让示例清楚展示优化效果，每个 block 至少输出：

- changed account 数；
- cache hits / misses；
- proof target 数；
- 返回的 proof node 数；
- reveal node 数；
- provider 兜底调用次数；
- event 数以及按类型分类的计数；
- pending/ready/inflight/completed 队列高水位；
- proof chunk 数、每个 chunk 的 target 数；
- stale completion 丢弃数；
- scheduler idle/dispatch/coalesce 次数；
- dirty/rehashed node 数；
- trie update upsert/delete 数；
- cache 当前 node 数；
- sparse root 与 full rebuild root 是否一致。

这些计数只用于观察，不参与正确性判断。

## 23. 推荐实现顺序

### Phase 1：接线 alloy-trie（天级，不是周级）

- Account RLP 编码（`alloy_trie::TrieAccount` 或手写四元组）；
- 有序 `HashedAccounts` + `HashBuilder` full-rebuild oracle；
- `HashBuilder` + `ProofRetainer` 生成 multiproof，并用 `proof::verify_proof` 自检；
- 内存 TrieDB（root 记录 + `TrieUpdates` 应用）。

### Phase 2：确定性事件循环骨架

- `Event`、generation 和 scheduler phase；
- event/work 两个 `VecDeque`；
- pending/inflight/completed bookkeeping；
- synchronous simulated proof worker；
- readiness、backpressure、drain 和 no-progress 检查；
- 先用假的 proof/result 跑通状态转换测试。

### Phase 3：集成 SerialSparseTrie

- `with_root` 冷启动 + `reveal_nodes` 灌入 multiproof；
- `update_leaf` / `remove_leaf`（删除延后）+ `root` + `take_updates`；
- provider 先用 `DefaultTrieNodeProvider` 占位；
- 与 full-rebuild oracle 对照。

### Phase 4：cache 与去重调度

- cache anchor；
- 跨 block 保留 revealed nodes；
- fetched-target 去重与 EmptyProof 短路；
- proof chunking、乱序 completion、sequencer 与 stale generation；
- 实现 `FlatStoreProvider` 兜底，覆盖 delete collapse 的 blinded sibling。

### Phase 5：Reth 2.0 hashed state 与历史状态

- HashedAccounts 成为唯一 latest state；
- plain-address changesets；
- account history index；
- historical lookup；
- atomic commit。

### Phase 6：unwind 与 pruning

- reverse trie updates 或 full rebuild unwind；
- cache invalidation；
- historical pruning；
- 可选 sparse cache pruning。

## 24. 完成定义

当以下条件全部满足时，可以认为这个示例完成：

- 不存在 plain latest-state table；
- latest account 通过 `keccak256(address)` 定位；
- cold cache 能通过 proof 完成 sparse update；
- warm cache 能复用已 reveal 节点；
- 已取过的 proof target 不再重复请求，EmptyProof 短路仍保持 sequence 顺序语义；
- 插入、更新、删除和 branch collapse 均通过 full rebuild root 校验；
- TrieUpdates 可以把持久化 TrieDB 推进到相同 root；
- historical account 查询严格遵守 `state(block)` 语义；
- unwind 后 hashed state、history 和 trie root 一致；
- history pruning 不影响 latest state；
- cache 的存在与否只影响 proof I/O，不影响任何结果；
- scheduler 是 working trie 的唯一 writer，proof worker 只能通过 completion event 返回结果；
- `FinishUpdates`、drain、root validation 和 atomic commit 是四个不同阶段；
- 乱序 completion、不同 chunk size 和 worker capacity 下结果仍 deterministic；
- cancel/reorg 后的 stale generation proof 不会污染新 cache；
- provider miss 作为硬错误立即失败退出，而不是永久自旋。

## 25. 参考

- [Releasing Reth 2.0](https://www.paradigm.xyz/writing/releasing-reth-2-0)
- [Reth Storage V2](https://reth.rs/run/faq/storage-v2/)
- [Reth: use hashed state as canonical state representation](https://github.com/paradigmxyz/reth/pull/21115)
- [Reth sparse trie implementation](https://github.com/paradigmxyz/reth/tree/main/crates/trie/sparse)
- [Reth proof v2 implementation](https://github.com/paradigmxyz/reth/tree/main/crates/trie/trie/src/proof_v2)
- [alloy-trie](https://github.com/alloy-rs/trie)（`HashBuilder`、`ProofRetainer`、`Nibbles`、proof 校验）
- upstream 调度参考：`crates/engine/tree/src/tree/payload_processor/multiproof.rs`（`MultiProofTask` / `ProofSequencer`）与 `sparse_trie.rs`（`SparseTrieTask`）
