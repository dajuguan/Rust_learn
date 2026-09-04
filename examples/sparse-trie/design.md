1. 输入update_rx是直接传递进来，还是在这里初始化后返回
2. 同理输入FinishedStateUpdates
    - 
3. 最后的结果怎么传递出去？
    - 给定一个单独的channel
4. overlay需要mutex吗？还是直接copy on write即可？
    - 直接take的


## todo
好问题！让我解释一下 reth 为什么需要那么多 `SeekResult` 判断，以及我们简化的影响：

## Reth 为什么需要 SeekResult？

### 1. Sparse Trie 的惰性特性

Sparse trie 是**惰性加载**的 - 节点只有在收到 proof 后才会被 reveal。所以在操作时可能遇到：

```
SeekResult::Blinded        → 遇到未 reveal 的节点，需要请求 proof
SeekResult::RevealedSubtrie → 遇到已 reveal 的 subtrie，路由到它
SeekResult::RevealedLeaf   → 遇到 leaf 节点
SeekResult::EmptyRoot      → 空 trie
SeekResult::Diverged       → path 分叉（需要 split branch）
SeekResult::NoChild        → 没有子节点
```

### 2. 需要处理的复杂情况

**update_leaves 时**：
- 遇到 blinded node → 需要请求 proof，暂停更新
- leaf 已存在 → 更新 value
- leaf 不存在 → 插入新 leaf，可能需要 split branch
- 删除 leaf → 可能需要 collapse branch

**reveal_nodes 时**：
- 节点已 reveal → 跳过
- 节点是 blinded → 替换为 proof node
- path 不匹配 → 跳过

### 3. Cursor 的作用

Reth 用 `ArenaCursor` 来：
- 跟踪当前遍历位置
- 维护父子关系
- 支持 seek、next 等操作
- 处理 dirty 状态传播

## 我们简化的影响

| 功能 | Reth | 我们 | 影响 |
|------|------|------|------|
| Blinded node 处理 | 请求 proof，暂停 | 返回错误或跳过 | ⚠️ 不能正确处理惰性加载 |
| Branch splitting | 完整实现 | 基本实现 | ⚠️ 某些插入可能不正确 |
| Branch collapsing | 完整实现 | 未实现 | ⚠️ 删除后 trie 结构不优化 |
| Extension nodes | 合并到 branch | 未处理 | ⚠️ 不支持 extension |
| Cursor 遍历 | 完整的 cursor | 简单 path 遍历 | ⚠️ 效率较低，但功能基本可用 |

## 简化的合理性

对于 **minimal 实现**，简化是可以的，因为：

1. **学习目标**：理解架构比处理所有 edge case 更重要
2. **核心流程已覆盖**：
   - ✅ 两层结构（upper + subtries）
   - ✅ 并行处理框架
   - ✅ Arena 内存管理
   - ✅ SparseTrie trait 实现

3. **可以后续迭代**：先有框架，再逐步完善

## 如果要改进

下一步可以添加：
1. 简单的 cursor 结构，跟踪遍历状态
2. 更好的 blinded node 处理（返回需要 proof 的列表）
3. Branch splitting 的基本实现

**结论**：当前的简化版本作为学习和演示架构是足够的。如果需要生产级别的正确性，才需要实现完整的 SeekResult 处理。