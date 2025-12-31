/* GAT核心:
GAT:使关联类型成为类型构造器。
GAT 本质上解决的是同一类问题：关联类型本身也需要带参数（生命周期 / 类型， 即关联类型不能是一个‘定值’，而必须是一个‘类型函数），而这些参数必须和 Self 的借用或泛型约束绑定。
1. 生命周期类问题: 返回的类型需要“借用 self”，且该借用的生命周期只能和某一次调用绑定，而不是和 Self 绑定。比如Async, 下面的LendingIterator。
2. 返回的关联类型自己再吃一个泛型参数： 比如reth db或者PointerFamily例子。


References:
- [The push for GATs stabilization](https://blog.rust-lang.org/2021/08/03/GATs-stabilization-push/)
- [GAT RFC](https://github.com/rust-lang/rfcs/blob/master/text/1598-generic_associated_types.md)
- [GAT explaination by nikomatsakis](https://www.youtube.com/watch?v=JwG-Wa7dOBU)
- [Async impl Trait in traits requires GATs](https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/)
- [GATs: Decide whether to have defaults for where Self: 'a](https://github.com/rust-lang/rust/issues/87479)
*/

use std::{io::Error, ops::Deref, sync::Arc};

/* =====================================================
Why GAT is a must? e.g., why iterator is enough?
It can't express the trait bound on Item and next<'a>.
 */
struct WindowIter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Iterator for WindowIter<'a> {
    type Item = &'a mut [u8];
    fn next<'b>(&'b mut self) -> Option<Self::Item> {
        let start = self.pos;
        let end = start + 2;
        self.pos += 1;
        // requires 'b: 'a, but we can't provide this!
        // Some(&mut self.buf[start..end])
        unimplemented!()
    }
}

/* =====================================================
Why where Self:x is a must
trait LendingIteratorNoClause {
    // we must ensure self type outlive 'a, so where clause is a must.
    type Item<'x>; // surpose it's correct, just ignore the warning
    fn next<'b>(&'b mut self) -> Self::Item<'b>;
}

struct RefOnce<T> {
    my_data: T,
}

impl<T> LendingIteratorNoClause for RefOnce<T> {
    type Item<'a> = &'a T;

    fn next<'b>(&'b mut self) -> Self::Item<'b> {
        &self.my_data
    }
}

fn boom<'x>(x: &'x i32) {
    let it = RefOnce { my_data: x };

    // It's legal to write this, which means T:'static is true, which it actually might not!
    // So where clause must be constrained to ensure the life time is correct.
    type Bad = <RefOnce<i32> as LendingIterator>::Item<'static>;
    // the above means:
    // type Bad = &'static &'x i32;
}
 */

/* =====================================================
Correct version
 */
trait LendingIterator {
    // we must ensure self type outlive 'a, so where clause is a must.
    // Item<'a> 是一个“类型构造器”，不是函数参数。
    // 错误直觉（大多数人都会有）Item<'x>中的'x 一定来自 next(&'b mut self)，但是实际上x'只是个类型参数和'b无关，除非指定where Self:'x。
    // 真实直觉: Item<'a> 是一个“公开的类型”，任何人、在任何地方，都可以用任何 'a(类型参数)来实例化它，该类型可以被“脱离 next() 使用。
    type Item<'x>
    where
        Self: 'x;
    fn wnext<'b>(&'b mut self) -> Option<Self::Item<'b>>;
}

impl<'a> LendingIterator for WindowIter<'a> {
    type Item<'x>
        = &'x mut [u8]
    where
        Self: 'x;

    fn wnext<'c>(&'c mut self) -> Option<Self::Item<'c>> {
        let start = self.pos;
        let end = start + 2;
        self.pos += 1;
        Some(&mut self.buf[start..end])
    }
}

/* =====================================================
Reth db中的教科书级别GAT使用
 */

/// Read only transaction
pub trait DbTx: Send {
    /// Cursor type for this read-only transaction
    type Cursor<T: Table>: DbCursorRO<T> + Send + Sync;
    /// Iterate over read only values in table. Use GAT to create a cursor type for each table.
    fn cursor_read<T: Table>(&self) -> Result<Self::Cursor<T>, Error>;
}

pub trait DbCursorRO<T: Table> {
    /// Positions the cursor at the first entry in the table, returning it.
    fn first(&mut self) -> Result<T, Error>;
}

pub trait Table: Send + Sync + 'static {
    /// The table's name.
    const NAME: &'static str;

    /// Whether the table is also a `DUPSORT` table.
    const DUPSORT: bool;

    /// Key element of `Table`.
    ///
    /// Sorting should be taken into account when encoding this.
    type Key: Key;

    /// Value element of `Table`.
    type Value: Key;
}

pub trait Key: Ord + Clone + for<'a> Deserialize<'a> {}

pub trait Deserialize<'de>: Sized {
    // Required method
    fn deserialize<D>(deserializer: D) -> Result<Self, Error>;
}

/*=====================================================
常见的FamilyPointer GAT: 关联类型generic over实现类型(Self) + 类型参数。
不论Arc, Box都能作为Generic，同时其中的字段本身又是一个Generic的类型。
 */

pub trait PointerFamily {
    type Pointer<T>: Deref<Target = T>; // f(t) => sometype(t)
    fn new<T>(value: T) -> Self::Pointer<T>;
}

#[derive(Debug)]
struct ArcFamily;

impl PointerFamily for ArcFamily {
    type Pointer<T> = Arc<T>; // like f(t) => Arc(t)
    fn new<T>(value: T) -> Self::Pointer<T> {
        Arc::new(value)
    }
}

// 注意: 虽然这么写也没问题，但是语义就不对了，我们希望是为任意的类型T，创建Arc<T>；
// 但是下面的写法只能为某个实例化的Arc<T>类型，比如Arc<i32>，创建Arc<i32>。
// Arc<T>表面看起来是泛型，但是实际上是实例化的泛型了，T会被单态化；而ArcFamily的语义是关联类型是类型函数，不是具体的类型。
impl<T> PointerFamily for Arc<T> {
    type Pointer<P> = Arc<P>;
    fn new<P>(value: P) -> Self::Pointer<P> {
        Arc::new(value)
    }
}

#[derive(Debug)]
struct BoxFamily;

impl PointerFamily for BoxFamily {
    type Pointer<T> = Box<T>; // like f(t) => box(t)
    fn new<T>(value: T) -> Self::Pointer<T> {
        Box::new(value)
    }
}

#[derive(Debug)]
struct Widget<P: PointerFamily> {
    int: P::Pointer<i32>,
    bo: P::Pointer<bool>,
}

type ArcWidget = Widget<ArcFamily>;
type ArcAWidget<T> = Widget<Arc<T>>;
type BoxWidget = Widget<BoxFamily>;

/* =====================================================
PointerFamily without GAT, will be more complex.
 */
pub trait PointerFamilyNoGAT<T> {
    type Pointer: Deref<Target = T>;
    fn new_no_gat(value: T) -> Self::Pointer;
}

impl<T> PointerFamilyNoGAT<T> for ArcFamily {
    type Pointer = Arc<T>;
    fn new_no_gat(value: T) -> Self::Pointer {
        Arc::new(value)
    }
}

impl<T> PointerFamilyNoGAT<T> for BoxFamily {
    type Pointer = Box<T>;
    fn new_no_gat(value: T) -> Self::Pointer {
        Box::new(value)
    }
}

#[derive(Debug)]
struct WidgetNoGAT<PI: PointerFamilyNoGAT<i32>, PB: PointerFamilyNoGAT<bool>> {
    int: PI::Pointer,
    bo: PB::Pointer,
}

type ArcWidgetNoGAT = WidgetNoGAT<ArcFamily, ArcFamily>;

#[test]
fn test_gat_lifetime_generic() {
    let mut buf = vec![0, 1, 2, 3];
    let mut iter = WindowIter {
        buf: &mut buf,
        pos: 0,
    };

    let item = iter.wnext();
    item.map(|v| {
        v[0] = 1;
    });

    let item = iter.wnext();
    item.map(|v| {
        v[0] = 5;
    });

    println!("buf:{:?}", buf);
}

#[test]
fn test_gat_generic() {
    let a = ArcFamily::new(1);
    let b = ArcFamily::new(false);
    let p: ArcWidget = Widget { int: a, bo: b };
    println!("{:?}", p);

    let a = BoxFamily::new(1);
    let b = BoxFamily::new(false);
    let p: BoxWidget = Widget { int: a, bo: b };
    println!("{:?}", p);
}

#[test]
fn test_no_gat_generic() {
    let a = ArcFamily::new(1);
    let b = ArcFamily::new(false);
    let p: ArcWidgetNoGAT = WidgetNoGAT { int: a, bo: b };
    println!("{:?}", p);
}
