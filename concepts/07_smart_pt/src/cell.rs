use std::cell::Cell;

#[test]
fn test_cell_withnocopy() {
    let arr = vec![1, 2, 3];
    let c = Cell::new(arr);
    // couldn't use get because vector can't be copied.
    // let b = c.get();
    let d = &c;
    let e = &c;
    d.set(vec![]);
    e.set(vec![1]);
    println!("d: {:?}", d.take());
}

#[test]
fn test_cell_basictype() {
    let var = 4;
    let c = Cell::new(var);
    // couldn't use get because vector can't be copied.
    // let b = c.get();
    let d = c.get();
    let e = &c;
    // we can still set c, even when reference to c 's lifetime hasn't end
    c.set(5);
    println!("c: {:?}, e:{:?}", c, e);
}
