//! Headless command/event recorder; files are debug artifacts, never saves.
fn main() {
    if let Err(e) = ws_cli::main_entry(std::env::args().skip(1).collect()) {
        eprintln!("ws-cli: {e}");
        std::process::exit(1);
    }
}
