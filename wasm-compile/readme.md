```
cargo install wasm-pack
wasm-pack build --release
/root/now/zkWasm/target/release/delphinus-cli -k 22 --function zkmain --wasm pkg/wasm_compile_bg.wasm dry-run --public 3:i64 
```