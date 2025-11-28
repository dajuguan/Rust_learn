/* 73. Set Matrix Zeroes
Given an m x n integer matrix matrix, if an element is 0, set its entire row and column to 0's.

You must do it in place.

Example 1:
    Input: matrix = [[1,1,1],[1,0,1],[1,1,1]]
    Output: [[1,0,1],[0,0,0],[1,0,1]]
Example 2:
    Input: matrix = [[0,1,2,0],[3,4,5,2],[1,3,1,5]]
    Output: [[0,0,0,0],[0,4,5,0],[0,3,1,0]]
 */

use crate::Solution;

impl Solution {
    pub fn set_zeroes(matric: &mut Vec<Vec<i32>>) {
        let m = matric.len();
        let n = matric[0].len();
        // save first row/col's mark to delay setting these as zeros, as we'll use it as marker.
        let first_row_zero = (0..n).any(|col| matric[0][col] == 0);
        let first_col_zero = (0..m).any(|row| matric[row][0] == 0);

        // use first col and row to in-place mark the zeroed col, rows
        for i in 1..m {
            for j in 1..n {
                if matric[i][j] == 0 {
                    matric[0][j] = 0;
                    matric[i][0] = 0;
                }
            }
        }

        for col in 1..n {
            if matric[0][col] == 0 {
                for row in 1..m {
                    matric[row][col] = 0;
                }
            }
        }

        for row in 1..m {
            if matric[row][0] == 0 {
                for col in 1..n {
                    matric[row][col] = 0;
                }
            }
        }

        // set first row and first col at last
        if first_row_zero {
            for col in 0..n {
                matric[0][col] = 0;
            }
        }

        if first_col_zero {
            for row in 0..m {
                matric[row][0] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_zeros() {
        let mut inputs = vec![
            vec![vec![1, 1, 1], vec![1, 0, 1], vec![1, 1, 1]],
            vec![vec![0, 1, 2, 0], vec![3, 4, 5, 2], vec![1, 3, 1, 5]],
        ];
        let outputs = vec![
            vec![vec![1, 0, 1], vec![0, 0, 0], vec![1, 0, 1]],
            vec![vec![0, 0, 0, 0], vec![0, 4, 5, 0], vec![0, 3, 1, 0]],
        ];

        // let inputs = vec!["dvdf".to_string()];
        // let outputs = vec![3];
        for (i, input) in inputs.iter_mut().enumerate() {
            Solution::set_zeroes(input);
            assert_eq!(outputs[i], input.to_vec());
        }
    }
}
