// Suppress the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--mcp") {
        if let Err(error) = pe_app::mcp::native::bridge() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(err) = pe_app::run() {
        eprintln!("PolarExplorer failed to start: {err:#}");
        std::process::exit(1);
    }
}
