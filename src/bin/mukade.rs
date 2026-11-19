//! `mukade list|dump|write|hop` — see [`mukade::cli`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let started = std::time::Instant::now();
    let result = mukade::cli::run(&args);
    let phases = mukade::timing::report();
    if !phases.is_empty() {
        eprint!("{phases}");
        eprintln!(
            "{:<28} {:>9.1} ms",
            "total",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    if let Err(error) = result {
        eprintln!("{error}");
        // 5.5.2: 2 refused with a reason, 4 failed and nothing changed, 5 failed and recovery needed.
        let code = error
            .downcast_ref::<mukade::HopError>()
            .map(mukade::HopError::exit_code)
            .unwrap_or(1);
        std::process::exit(code);
    }
}
