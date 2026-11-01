//! lazybw — the binary.

use color_eyre::eyre::Result;

fn main() -> Result<()> {
    color_eyre::install()?;
    lazybw::security::harden_process();
    lazybw::cli::run()
}
