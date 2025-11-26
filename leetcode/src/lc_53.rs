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
