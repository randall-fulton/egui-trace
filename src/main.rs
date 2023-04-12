#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Tracing",
        options,
        Box::new(|_cc| Box::<egui_trace::App>::default()),
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}
