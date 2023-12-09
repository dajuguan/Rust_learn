
trait ShapeFn {
    fn perimeter(&self) -> f64;
}

struct Rectangle {
    a: f64,
    b: f64,
}

impl ShapeFn for Rectangle {
    fn perimeter(&self) -> f64 {
        self.a + self.b
    }
}


struct Triangle {
    a: f64,
    b: f64,
    c: f64,
}

impl ShapeFn for Triangle {
    fn perimeter(&self) -> f64 {
        self.a + self.b + self.c
    }
}

#[allow(dead_code)]
enum Shape {
    Rect(Rectangle),
    Tria(Triangle)
}

impl ShapeFn for Shape {
    fn perimeter(&self) -> f64 {
        match self {
            Shape::Rect(r) => r.perimeter(),
            Shape::Tria(t) => t.perimeter()
        }
    }
}

fn main() {
    // reference type & pointer type
    let num = 5;
    let r1 = &num;
    let r2 = &num as *const i32;

    println!("r1: {:p},{}", r1, *r1);
    unsafe {
        println!("r2: {:p},{}", r2, *r2);
    }

    // trait
    let s1 = Rectangle {a: 2.0, b: 3.0};
    let s2 = Triangle {a: 2.0, b: 3.0, c: 4.0};

    let c = Shape::Rect(s1);
    println!("c perimeter: {}", c.perimeter());
    let c = Shape::Tria(s2);
    println!("c perimeter: {}", c.perimeter());
}
