
pub mod app;
pub mod common;
pub mod defines;
pub mod wallet;

fn main() {
    if let Err(error) = app::WalletIntruder::main() {
        eprintln!();

        tracing::error!("{error}\n");

        eprintln!("Press Enter to exit...");
        std::io::stdin().read_line(&mut String::new()).unwrap();

        std::process::exit(1);
    }
}
