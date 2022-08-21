
use std::{sync::{mpsc, Arc, atomic::{AtomicBool, Ordering}}, thread};

use crypto::{sha2::Sha256, digest::Digest};

const BASE:usize = 32;
static DIFICULTY: &'static str  = "000000";
#[derive(Debug)]
struct Solution {
    num:usize,
    hash:String
}
fn verify(number:usize, ) -> Option<Solution>{
    let mut hasher = Sha256::new();
    hasher.input_str(&(number * BASE).to_string());
    let hash = hasher.result_str();
    if hash.starts_with(DIFICULTY) {
        Some(Solution{num:number,hash})
    } else {
        None
    }
}

fn find(start_at:usize, finded_solution:Arc<AtomicBool> ,step:usize ,sender:mpsc::Sender<Solution>){
    for num in (start_at..).step_by(step){
        if finded_solution.load(Ordering::Relaxed) {
            return;
        }
        if let Some(s)= verify(num) {
            finded_solution.store(true, Ordering::Relaxed);
            sender.send(s).unwrap();
            return;
        }
    }
}
fn main() {

    let thread_num = 8;
    let (sx, rx) = mpsc::channel();
    let finded_solution = Arc::new(AtomicBool::new(false));
    for i in 0..thread_num {
        let sender = sx.clone();
        let finded_solution = finded_solution.clone();
        thread::spawn(move ||{
             find(i,finded_solution,thread_num,sender);
        });
    }
    match rx.recv() {
        Ok(s) => println!("{:?}", s),
        Err(_) => panic!("Worker thread detached!")
    } 
}

