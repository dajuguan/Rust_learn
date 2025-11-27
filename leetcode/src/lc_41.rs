/* 41. First Missing Positive
Given an unsorted integer array nums. Return the smallest positive integer that is not present in nums.

You must implement an algorithm that runs in O(n) time and uses O(1) auxiliary space.

Example 1:

Input: nums = [1,2,0]
Output: 3
Explanation: The numbers in the range [1,2] are all in the array.
Example 2:

Input: nums = [3,4,-1,1]
Output: 2
Explanation: 1 is in the array but 2 is missing.
Example 3:

Input: nums = [7,8,9,11,12]
Output: 1
Explanation: The smallest positive integer 1 is missing.
 */

use crate::Solution;

impl Solution {
    pub fn first_missing_positive_simple_to_understand(mut nums: Vec<i32>) -> i32 {
        for i in 0..nums.len() {
            if nums[i] != i as i32 + 1 {
                let mut cur = nums[i];
                loop {
                    if cur > 0 && cur <= i as i32 + 1 {
                        nums[cur as usize - 1] = cur;
                        break;
                    } else if cur > i as i32 + 1
                        && cur <= nums.len() as i32
                        && nums[cur as usize - 1] != cur
                    {
                        let temp = nums[cur as usize - 1];
                        nums[cur as usize - 1] = cur;
                        cur = temp;
                    } else {
                        break;
                    }
                }
            }
        }

        for (i, &num) in nums.iter().enumerate() {
            if num != i as i32 + 1 {
                return i as i32 + 1;
            }
        }
        nums.len() as i32 + 1
    }
    pub fn first_missing_positive(mut nums: Vec<i32>) -> i32 {
        let n = nums.len();
        let mut i = 0;

        while i < n {
            let x = nums[i];
            // target index
            if x > 0 && x <= n as i32 {
                let t = x as usize - 1;
                // already in correct place or would create a cycle
                if nums[t] == x {
                    i += 1;
                    continue;
                }
                // move nums[i] → position t, and take nums[t] into nums[i]
                let temp = nums[t];
                nums[t] = x;
                nums[i] = temp;

                // do NOT increment i here ― we must validate the new nums[i]
                continue;
            }

            i += 1;
        }

        // find the first missing
        for (i, &v) in nums.iter().enumerate() {
            if v != i as i32 + 1 {
                return i as i32 + 1;
            }
        }

        n as i32 + 1
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::Solution;

    #[test]
    fn test_first_missing_positive() {
        let inputs = vec![
            vec![1, 2, 0],
            vec![3, 4, -1, 1],
            vec![-1],
            vec![2, 3, 1],
            vec![7, 8, 9, 11],
            vec![2, 2],
        ];
        let outputs = vec![3, 2, 1, 4, 1, 1];

        // let inputs = vec![vec![2, 2]];
        // let outputs = vec![1];
        for (i, input) in inputs.iter().enumerate() {
            let res = Solution::first_missing_positive(input.to_vec());
            assert_eq!(res, outputs[i]);
        }
    }
}
