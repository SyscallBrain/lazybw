//! `fake-bw` binary entry point. The implementation lives in the library.

#![deny(unsafe_code)]

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = fake_bw::run(&args);
    std::process::ExitCode::from(u8::try_from(code).unwrap_or(1))
}
