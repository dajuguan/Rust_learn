# Rust每日一题---数据结构-数组movezeros
[leetcode地址](https://leetcode.cn/problems/move-zeroes/)
## 题目描述
给定一个数组 nums，编写一个函数将所有 0 移动到数组的末尾，同时保持非零元素的相对顺序。

请注意 ，必须在不复制数组的情况下原地对数组进行操作。

**示例 1**:
> 输入: nums = [0,1,0,3,12]

> 输出: [1,3,12,0,0]

**示例 2**:
> 输入: nums = [0]

> 输出: [0]

## 知识点
- 可变引用vector修改
- vector的slice由于是动态数组，无法在编译时确定长度不能直接赋值
- num.iter()返回的是不可变引用，不能在其中更改元素

## 思路
由于不能通过slice和iter进行函数式的操作，只能进行遍历，设置关键flag
1. 从前到后遍历数组的index，把不为零的往前移,并记录不为零的元素个数j；
2. 从j到arr.len()将剩下的元素设为0

## leecode代码

```
impl Solution {
    pub fn move_zeroes(nums: &mut Vec<i32>) {
        let mut j = 0;
        let mut i = 0;
        let len = nums.len();
        while i<len {
            if nums[i] != 0 {
                nums[j] = nums[i];
                j += 1;
            }
            i +=1;
        }
        while j < len {
            nums[j] = 0;
            j += 1;
        }
    }
}
```