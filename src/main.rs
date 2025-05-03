use std::env;

use anyhow::anyhow;
use worf_lib::config::Mode;
use worf_lib::{config, mode};
fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    if let Some(show) = &config.show() {
        match show {
            Mode::Run => {
                mode::run(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Drun => {
                mode::d_run(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Dmenu => {
                mode::dmenu(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::File => {
                mode::file(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Math => {
                mode::math(&config);
            }
            Mode::Ssh => {
                mode::ssh(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Emoji => {
                mode::emoji(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Auto => {
                mode::auto(&config).map_err(|e| anyhow!(e))?;
            }
        }

        Ok(())
    } else {
        Err(anyhow!("No mode provided"))
    }
}
