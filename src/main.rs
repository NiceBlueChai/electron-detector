//! Starts the Electron detector CLI process.

fn main() {
    match electron_detector::run(std::env::args()) {
        Ok(output) => print!("{output}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
