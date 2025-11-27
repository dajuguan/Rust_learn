/* Maximum Subarray
Given an integer array nums, find the subarray with the largest sum, and return its sum.

Example 1:

Input: nums = [-2,1,-3,4,-1,2,1,-5,4]
Output: 6
Explanation: The subarray [4,-1,2,1] has the largest sum 6.
Example 2:

Input: nums = [1]
Output: 1
Explanation: The subarray [1] has the largest sum 1.
Example 3:

Input: nums = [5,4,-1,7,8]
Output: 23
Explanation: The subarray [5,4,-1,7,8] has the largest sum 23.
 */
use crate::Solution;

impl Solution {
    pub fn max_sub_array(nums: Vec<i32>) -> i32 {
        let mut pre = 0;
        let mut ans = nums[0];
        for &num in nums.iter() {
            // f(i): max consecutive arr end with index i
            // f(i) = max{nums[i]+f(i-1), nums[i]}
            pre = num.max(pre + num);
            ans = ans.max(pre);
        }
        ans
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::Solution;

    #[test]
    fn test_max_sub_array() {
        let inputs = vec![vec![1], vec![-1], vec![-2, -1], vec![5, 4, -1, 7, 8]];
        let outputs = vec![1, -1, -1, 23];

        // let inputs = vec![vec![5, 4, -1, 7, 8]];
        // let outputs = vec![23];
        for (i, input) in inputs.iter().enumerate() {
            let res = Solution::max_sub_array(input.to_vec());
            assert_eq!(res, outputs[i]);
        }
    }
}
