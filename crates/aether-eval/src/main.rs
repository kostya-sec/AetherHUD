fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(aether_eval::cli::run() as u8)
}
