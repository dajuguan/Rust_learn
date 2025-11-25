/* 3. Longest Substring Without Repeating Characters
Given a string s, find the length of the longest substring without duplicate characters.



Example 1:

Input: s = "abcabcbb"
Output: 3
Explanation: The answer is "abc", with the length of 3. Note that "bca" and "cab" are also correct answers.
Example 2:

Input: s = "bbbbb"
Output: 1
Explanation: The answer is "b", with the length of 1.
Example 3:

Input: s = "pwwkew"
Output: 3
Explanation: The answer is "wke", with the length of 3.
Notice that the answer must be a substring, "pwke" is a subsequence and not a substring.
 */

use crate::Solution;

impl Solution {
    pub fn length_of_longest_substring(s: String) -> i32 {
        if s.len() <= 1 {
            return s.len() as i32;
        }
        let chars = s.chars();
        let mut last_indexs = std::collections::HashMap::new();
        let mut left = 0;
        let mut ans = 0;
        for (i, ch) in chars.enumerate() {
            let last_index = last_indexs.get(&ch);
            if let Some(&index) = last_index {
                if index >= left {
                    ans = ans.max(i - left);
                    left = index + 1;
                }
            }

            last_indexs.insert(ch, i);
        }
        ans = ans.max(s.len() - left);

        ans as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_of_longest_substring() {
        let inputs = vec![
            "".to_string(),
            "a".to_string(),
            "abcabcbb".to_string(),
            "bbbbb".to_string(),
            "pwwkew".to_string(),
            "au".to_string(),
            "dvdf".to_string(),
        ];
        let outputs = vec![0, 1, 3, 1, 3, 2, 3];

        // let inputs = vec!["dvdf".to_string()];
        // let outputs = vec![3];
        for (i, input) in inputs.iter().enumerate() {
            let res = Solution::length_of_longest_substring(input.to_string());
            assert_eq!(res, outputs[i]);
        }
    }
}
