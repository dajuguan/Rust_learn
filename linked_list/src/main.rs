type Link<T> = Option<Box<Node<T>>>;

struct  Node<T> {
    elem: T,
    next: Link<T>
}

struct LinkedList<T>{
    head: Link<T>
}

impl <T> LinkedList<T> {
    fn push(&mut self, val: T) {
        let new_node = Node {elem: val, next: self.head.take() };
        self.head = Some(Box::new(new_node)); 
    }

    fn pop(&mut self) -> Option<T>{
        self.head.take().map(|node|{
            self.head = node.next;
            node.elem
        })
    }

    fn peek<'a>(&'a self) -> Option<&'a T> {
        self.head.as_deref().map(|node|{
            &node.elem
        })
    }

    fn peek_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        self.head.as_deref_mut().map(|node| {
            &mut node.elem
        })
    }

    fn iter<'a>(&'a self) -> Iter<'a, T> {
        Iter {next: self.head.as_deref()}
    }

    fn iter_mut<'a>(&'a mut self) -> IterMut<'a, T> {
        IterMut { next: self.head.as_deref_mut() }
    }
       
}


struct Iter<'a, T> {
    next: Option<&'a Node<T>>
}

impl <'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.map(|node|{
            self.next = node.next.as_deref();
            &node.elem
        })
    }
}

struct  IterMut<'a, T> {
    next: Option<&'a mut Node<T>>
}

impl <'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next.take().map(|node|{
            self.next = node.next.as_deref_mut();
            &mut node.elem
        })
    }
}

impl <T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        let mut cur_link = self.head.take();
        while let Some(mut node) = cur_link {
            cur_link = node.next.take();
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn push_pop(){
        let mut list = LinkedList{head: None};
        list.push(3);
        list.push(2);
        assert_eq!(list.pop(), Some(2));
        assert_eq!(list.pop(), Some(3));
        assert_eq!(list.pop(), None);
    }

    #[test]
    fn peek(){
        let mut list = LinkedList{head: None};
        list.push(3);
        list.push(2);
        assert_eq!(list.peek(), Some(&2));
        list.peek_mut().map(|val|{
            *val = 4;
        });
        assert_eq!(list.peek(), Some(&4));
    }

    #[test]
    fn iter(){
        let mut list = LinkedList{head: None};
        list.push(3);
        list.push(2);
        let mut iter = list.iter();
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&3));
    }

    
    #[test]
    fn iter_mut(){
        let mut list = LinkedList{head: None};
        list.push(3);
        list.push(2);
        let mut iter = list.iter_mut();
        assert_eq!(iter.next(), Some(&mut 2));
        iter.next().map(|val|{
            *val = 5;
        });
        let mut iter = list.iter();
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&5));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_drop(){
        let mut list = LinkedList{head: None};
        for i in 0..1000000 {
            list.push(i);
        }
    }
}

fn main() {
    let a: usize = 4;
    let b: usize = 2;
    let a= vec![1,2,3,4,5];
    println!("{:?}",&a[1..6]);
}