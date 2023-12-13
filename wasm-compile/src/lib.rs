extern "C" {
    pub fn wasm_input(is_public: u32) -> u64;
    pub fn require(cond: bool);
}

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn zkmain() -> i64 {
    unsafe {
        let input = wasm_input(1);
        return 2 * input as i64;
    }
}