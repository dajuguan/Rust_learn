// Definition for singly-linked list.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct ListNode {
    pub val: i32,
    pub next: Option<Box<ListNode>>,
}

impl ListNode {
    #[inline]
    fn new(val: i32) -> Self {
        ListNode { next: None, val }
    }
}

use crate::Solution;

impl Solution {
    pub fn add_two_numbers(
        l1: Option<Box<ListNode>>,
        l2: Option<Box<ListNode>>,
    ) -> Option<Box<ListNode>> {
        // must sum one by one, or it'll overflow
        let mut p1 = &l1;
        let mut p2 = &l2;
        let mut head = Box::new(ListNode::new(0));
        let mut phead = &mut head;
        let mut carry = 0;

        while p1.is_some() || p2.is_some() || carry != 0 {
            let mut sum = carry;
            if let Some(node) = p1 {
                p1 = &node.next;
                sum += node.val;
            };
            if let Some(node) = p2 {
                p2 = &node.next;
                sum += node.val;
            };
            carry = sum / 10;
            phead.next = Some(Box::new(ListNode::new(sum % 10)));
            phead = phead.next.as_mut().unwrap();
        }

        head.next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_of_longest_substring() {
        let l1 = ListNode { val: 3, next: None };
        let l1 = ListNode {
            val: 4,
            next: Some(Box::new(l1)),
        };
        let l1 = ListNode {
            val: 2,
            next: Some(Box::new(l1)),
        };

        let l2 = ListNode { val: 4, next: None };
        let l2 = ListNode {
            val: 6,
            next: Some(Box::new(l2)),
        };
        let l2 = ListNode {
            val: 5,
            next: Some(Box::new(l2)),
        };

        let output = ListNode { val: 8, next: None };
        let output = ListNode {
            val: 0,
            next: Some(Box::new(output)),
        };
        let output = ListNode {
            val: 7,
            next: Some(Box::new(output)),
        };

        let l1_1 = ListNode { val: 0, next: None };
        let l2_1 = ListNode { val: 0, next: None };
        let output_1 = ListNode { val: 0, next: None };

        let l1_2 = ListNode { val: 9, next: None };
        let l2_2 = ListNode { val: 9, next: None };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 9,
            next: Some(Box::new(l2_2)),
        };
        let l2_2 = ListNode {
            val: 1,
            next: Some(Box::new(l2_2)),
        };
        let output_2: ListNode = ListNode { val: 1, next: None };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };
        let output_2: ListNode = ListNode {
            val: 0,
            next: Some(Box::new(output_2)),
        };

        let inputs = vec![(l1, l2), (l1_1, l2_1), (l1_2, l2_2)];
        let outputs = vec![output, output_1, output_2];

        // let inputs = vec!["dvdf".to_string()];
        // let outputs = vec![3];
        for (i, (l1, l2)) in inputs.iter().enumerate() {
            let l1 = Some(Box::new(l1.clone()));
            let l2 = Some(Box::new(l2.clone()));
            let res = Solution::add_two_numbers(l1, l2);
            assert_eq!(*res.unwrap(), outputs[i]);
        }
    }
}
