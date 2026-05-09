fn main() {
    if let Err(e) = skipper_db::cli::Cli::run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
