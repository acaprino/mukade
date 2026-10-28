//! `mukade list|dump|write|hop` — see [`mukade::cli`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = mukade::cli::run(&args) {
        eprintln!("{error}");
        // 5.5.2: 2 refused with a reason, 4 failed and nothing changed, 5 failed and recovery needed.
        let code = error
            .downcast_ref::<mukade::HopError>()
            .map(mukade::HopError::exit_code)
            .unwrap_or(1);
        std::process::exit(code);
    }
}
