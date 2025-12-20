fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<String>>();
    if args.len() < 2 {
        //cargo run -- "https://www.rust-lang.org/" "rust.md"
        panic!("Usage: cargo run -- <ur> <file>")
    }
    println!("args: {:?}", args);
    println!("Hello, world!");
    let url = &args[0];
    let output = &args[1];
    println!("Fetching urs: {}", url);
    let body = reqwest::blocking::get(url)?.text()?;
    println!("Converting to md...");
    let md = html2md::parse_html(&body);
    println!("write to file:{}", output);
    std::fs::write(output, md.as_bytes())?;
    Ok(())
}
