/* 11. Container With Most Water
You are given an integer array height of length n. There are n vertical lines drawn such that the two endpoints of the ith line are (i, 0) and (i, height[i]).

Find two lines that together with the x-axis form a container, such that the container contains the most water.

Return the maximum amount of water a container can store.

Notice that you may not slant the container.
 */

use crate::Solution;

impl Solution {
    pub fn max_area(height: Vec<i32>) -> i32 {
        let mut left = 0;
        let mut right = height.len() - 1;
        let mut ans = 0;
        while left < right {
            let width = (right - left) as i32;
            ans = ans.max(width * height[left].min(height[right]));
            // Use recursion to ensure that if we stop iterating at any step, the value will not exceed the current one, thereby reducing the problem size.
            if height[left] < height[right] {
                left += 1;
            } else {
                right -= 1;
            }
        }
        ans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_area() {
        let inputs = vec![
            vec![1, 8, 6, 2, 5, 4, 8, 3, 7],
            vec![1, 1],
            vec![8, 7, 2, 1],
        ];
        let outputs = vec![49, 1, 7];
        for (i, input) in inputs.iter().enumerate() {
            let ans = Solution::max_area(input.to_vec());
            assert_eq!(ans, outputs[i])
        }
    }
}
