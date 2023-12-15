pub struct Items<I> {
    iter: I,
    n: usize
}

impl<I> Iterator for Items<I> 
where I: Iterator{
    type Item = Vec<I::Item>;
    fn next(&mut self) -> Option<Vec<<I as Iterator>::Item>> {
        // self.map(f)
        let mut v = Vec::new();
        for _ in 0..self.n {
            if let Some(item) = self.iter.next() {
                v.push(item);
            }
        }

        Some(v)
    }
}

pub trait WindowExt {
    fn window_count(self, n: usize) -> Items<Self>
    where Self:Sized 
    {
        Items { iter: self, n }
    } 
}

impl<T: ?Sized> WindowExt for T where T:Iterator  {}