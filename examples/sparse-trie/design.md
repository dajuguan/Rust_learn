1. 输入update_rx是直接传递进来，还是在这里初始化后返回
2. 同理输入FinishedStateUpdates
    - 
3. 最后的结果怎么传递出去？
    - 给定一个单独的channel
4. overlay需要mutex吗？还是直接copy on write即可？
    - 直接take的