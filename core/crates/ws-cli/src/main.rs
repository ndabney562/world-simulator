//! Headless Vertical Slice runner (ADR 0040).
fn main() {
    if let Err(e) = ws_cli::main_entry(std::env::args().skip(1).collect()) {
        eprintln!("ws-cli: {e}");
        std::process::exit(1);
    }
}
