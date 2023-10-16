type List<T> = Option<Box<Node<T>>>;
struct LinkedList<T> {
    head: List<T>
}

#[derive(Clone)]
struct Node<T> {
    elem: T,
    next: List<T>
}

impl<T> LinkedList<T> {
    fn push(&mut self, val:T) {
        let new_node = Node {elem: val, next: self.head.take()} ;
        self.head = Some(Box::new(new_node));
    }

    fn pop(&mut self) -> Option<T> {
        let node = self.head.take();
        match node {
            Some(n) => {
                self.head = n.next;
                Some(n.elem)
            },
            None => None
        }
    }

    fn peek(&self) -> Option<&T> {
        self.head.as_deref().map(|node| {
            &node.elem
        })
    }

    fn peek_mut(&mut self) -> Option<&mut T> {
        self.head.as_deref_mut().map(|node| {
            &mut node.elem
        }) 
    }

    fn into_iter(self) -> IntoIter<T> {
        IntoIter(self)
    }

    fn iter<'a>(&'a self) -> Iter<'a, T> {
        Iter {next: self.head.as_deref()}
    }

    fn iter_mut<'a>(&'a mut self) -> IterMut<'a, T>{
        IterMut { next: self.head.as_deref_mut() }
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut cur_link = self.head.take();
        while let Some(mut node) = cur_link {
            cur_link = node.next.take();  //drop node
        }
    }
}


/// into Iter: T
struct IntoIter<T>(LinkedList<T>);

impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop()
    }
}

/// Iter: &T
struct Iter<'a, T>{
    next:Option<&'a Node<T>> 
}

impl <'a, T> Iterator for Iter<'a,T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.map(|node|{
            self.next = node.next.as_deref();
            &node.elem
        })
    }
}

//Itermut: &mut T

struct IterMut<'a, T> {
    next: Option<&'a mut Node<T>> 
}

impl <'a, T> Iterator for IterMut<'a,T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node| {
            self.next = node.next.as_deref_mut();
            &mut node.elem
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_list() {
        let mut list = LinkedList {head: None};
        list.push(1);
        list.push(2);
        list.push(3);
        assert_eq!(list.pop(), Some(3));
        assert_eq!(list.peek(), Some(&2));
        list.peek_mut().map(|val| *val = 4);
        assert_eq!(list.peek(), Some(&4));
        assert_eq!(list.pop(), Some(4));
        assert_eq!(list.pop(), Some(1));
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn test_intoiter() {
        let mut list = LinkedList {head: None};
        list.push(1);
        list.push(2);
        list.push(3);
        let mut iter = list.into_iter();
        assert_eq!(iter.next(), Some(3));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(1));
    }

    #[test]
    fn test_iter() {
        let mut list = LinkedList {head: None};
        list.push(1);
        list.push(2);
        list.push(3);
        let mut iter = list.iter();
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&1));
    }

    #[test]
    fn test_iter_mut() {
        let mut list = LinkedList {head: None};
        list.push(1);
        list.push(2);
        list.push(3);
        let mut iter = list.iter_mut();
        assert_eq!(iter.next(), Some(&mut 3));
        let v =  iter.next();
        v.map(|v| {
            *v = 4
        });
        assert_eq!(iter.next(), Some(&mut 1));
        let mut iter = list.iter();
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), Some(&4));
        assert_eq!(iter.next(), Some(&1));
    }
}

fn main() {
}