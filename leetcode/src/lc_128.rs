/*  longest consecutive sequence
Given an unsorted array of integers nums, return the length of the longest consecutive elements sequence.

You must write an algorithm that runs in O(n) time.



Example 1:

Input: nums = [100,4,200,1,3,2]
Output: 4
Explanation: The longest consecutive elements sequence is [1, 2, 3, 4]. Therefore its length is 4.
Example 2:

Input: nums = [0,3,7,2,5,8,4,6,0,1]
Output: 9
Example 3:

Input: nums = [1,0,1,2]
Output: 3
 */

pub fn longest_consecutive_initial(nums: Vec<i32>) -> i32 {
    let mut output = 0;
    let mut start_num = std::collections::HashMap::new();

    for start in nums.iter() {
        start_num.insert(start, (false, 1));
    }

    let keys = start_num.keys().map(|key| *key).collect::<Vec<_>>();

    for key in keys {
        let mut current_output = 1;
        let mut start_key = *key + 1;
        loop {
            if let Some((accessed, num_consecutive)) = start_num.get_mut(&start_key)
                && !*accessed
            {
                current_output += *num_consecutive;
                start_key += 1;
                *accessed = true;
            } else {
                break;
            }
        }

        println!("fetch key:{key}");

        if let Some((accessed, num_consecutive)) = start_num.get_mut(key) {
            *num_consecutive = current_output;
            println!("key:{key}, nums_consecutives:{num_consecutive}");
        }

        if current_output > output {
            output = current_output;
        }
    }

    output
}

pub fn longest_consecutive(nums: Vec<i32>) -> i32 {
    let mut output = 0;
    let start_num = nums.into_iter().collect::<std::collections::HashSet<_>>();

    // memory access to register will provide 50% performance increasement!
    for &key in &start_num {
        if start_num.contains(&(key - 1)) {
            continue;
        }
        let mut start_key = key + 1;
        while start_num.contains(&start_key) {
            start_key += 1;
        }

        output = output.max(start_key - key);
    }

    output
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    #[test]
    fn test_longest_consecutive() {
        let inputs = vec![
            vec![],
            vec![100],
            vec![100, 4, 200, 1, 3, 2],
            vec![0, 3, 7, 2, 5, 8, 4, 6, 0, 1],
            vec![1, 0, 1, 2],
            vec![0, 0, 1, -1],
        ];
        let expected_outputs = vec![0, 1, 4, 9, 3, 3];

        let inputs = vec![vec![0, 0, 1, -1]];
        let expected_outputs = vec![3];

        for (i, input) in inputs.iter().enumerate() {
            let res = longest_consecutive(input.to_vec());
            assert_eq!(res, expected_outputs[i], "input:{:?}", input);
        }
    }
}
