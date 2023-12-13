extern "C" {
    pub fn wasm_input(is_public: u32) -> u64;
    pub fn require(cond: bool);
    pub fn wasm_dbg(val:u64);
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn zkmain() -> i64 {
    unsafe {
        let input = wasm_input(1);
        wasm_dbg(input as u64);
        return 2 * input as i64;
    }
}