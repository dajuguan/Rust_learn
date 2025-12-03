/* 230. Kth Smallest Element in a BST
Given the root of a binary search tree, and an integer k, return the kth smallest value (1-indexed) of all the values of the nodes in the tree.
 */

use crate::Solution;

// Definition for a binary tree node.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub val: i32,
    pub left: Option<Rc<RefCell<TreeNode>>>,
    pub right: Option<Rc<RefCell<TreeNode>>>,
}

impl TreeNode {
    #[inline]
    pub fn new(val: i32) -> Self {
        TreeNode {
            val,
            left: None,
            right: None,
        }
    }
}
use std::cell::RefCell;
use std::rc::Rc;
impl Solution {
    pub fn kth_smallest(root: Option<Rc<RefCell<TreeNode>>>, mut k: i32) -> i32 {
        let mut root = root;
        let mut stack = vec![];
        while root.is_some() || stack.len() > 0 {
            while let Some(node) = root {
                // Rc::clone is faster than node.clone() because there is no need to unwarp option then clone with option, it just increase the reference counter.
                stack.push(Rc::clone(&node));
                root = node.borrow().left.clone();
            }
            let node = stack.pop().unwrap();
            k -= 1;
            if k == 0 {
                return node.borrow().val;
            }
            root = node.borrow().right.clone();
        }

        root.unwrap().borrow().val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kth_smallest() {
        let right = TreeNode {
            val: 2,
            left: None,
            right: None,
        };
        let left = TreeNode {
            val: 1,
            left: None,
            right: Some(Rc::new(RefCell::new(right))),
        };
        let right = TreeNode {
            val: 4,
            left: None,
            right: None,
        };
        let root = TreeNode {
            val: 3,
            left: Some(Rc::new(RefCell::new(left))),
            right: Some(Rc::new(RefCell::new(right))),
        };
        let t1 = Some(Rc::new(RefCell::new(root)));
        let inputs = vec![t1];
        let outputs = vec![2];

        // let inputs = vec!["dvdf".to_string()];
        // let outputs = vec![3];
        for (i, input) in inputs.iter().enumerate() {
            let res = Solution::kth_smallest(input.clone(), 2);
            assert_eq!(res, outputs[i]);
        }
    }
}
