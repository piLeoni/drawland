//! `drawland map`: a profile and a place in, the layered map SVG out. The
//! command line itself is `drawland::cli`.

fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(drawland::cli::run(std::env::args_os()) as u8)
}
