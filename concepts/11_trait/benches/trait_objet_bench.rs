use criterion::{Criterion, black_box, criterion_group, criterion_main};

pub trait Executor {
    fn run(&self) -> i32;
}
pub fn execute_generics(cmd: &impl Executor) -> i32 {
    cmd.run()
}

pub fn execute_trait_obj(cmd: &dyn Executor) -> i32 {
    cmd.run()
}

pub fn execute_boxed_trait_obj(cmd: Box<dyn Executor>) -> i32 {
    cmd.run()
}

pub struct SimpleExe {
    age: i32,
}

impl Executor for SimpleExe {
    fn run(&self) -> i32 {
        return self.age;
    }
}

//---------------------------benchmark functions-------------------//
pub fn generics_benchmark(c: &mut Criterion) {
    c.bench_function("generics", |b| {
        b.iter(|| {
            let cmd = SimpleExe { age: 1 };
            execute_generics(black_box(&cmd));
        })
    });
}

pub fn trait_object_benchmark(c: &mut Criterion) {
    c.bench_function("trait_object", |b| {
        b.iter(|| {
            let cmd = SimpleExe { age: 1 };
            execute_trait_obj(black_box(&cmd));
        })
    });
}

pub fn boxed_object_benchmark(c: &mut Criterion) {
    c.bench_function("boxed_trait_object", |b| {
        b.iter(|| {
            let cmd = Box::new(SimpleExe { age: 1 });
            execute_boxed_trait_obj(black_box(cmd));
        })
    });
}

criterion_group!(
    benches,
    generics_benchmark,
    trait_object_benchmark,
    boxed_object_benchmark
);
criterion_main!(benches);

/*
generics > 10x trait object -> 8x boxed trait object
so, generic is 80 times faster than boxed trait object
-----------------------------------------------------------------------------
generics                time:   [101.83 ps 102.19 ps 102.57 ps]
                        change: [-0.2877% +0.5883% +1.4304%] (p = 0.18 > 0.05)
                        No change in performance detected.
Found 3 outliers among 100 measurements (3.00%)
  2 (2.00%) high mild
  1 (1.00%) high severe

trait_object            time:   [1.0064 ns 1.0107 ns 1.0156 ns]
                        change: [+23.074% +24.437% +25.917%] (p = 0.00 < 0.05)
                        Performance has regressed.
Found 7 outliers among 100 measurements (7.00%)
  3 (3.00%) high mild
  4 (4.00%) high severe

boxed_trait_object      time:   [7.7167 ns 7.7616 ns 7.8089 ns]
                        change: [+0.8992% +1.5356% +2.1976%] (p = 0.00 < 0.05)
                        Change within noise threshold.
*/
