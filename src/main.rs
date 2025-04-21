use std::env;

use anyhow::anyhow;
use worf_lib::config::Mode;
use worf_lib::{config, mode};

fn main() -> anyhow::Result<()> {
    gtk4::init()?;

    env_logger::Builder::new()
        // todo change to error as default
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .init();

    let args = config::parse_args();
    let mut config = config::load_config(Some(args)).map_err(|e| anyhow!(e))?;

    if let Some(show) = &config.show {
        match show {
            Mode::Run => {
                todo!("run not implemented")
            }
            Mode::Drun => {
                mode::d_run(&config)?;
            }
            Mode::Dmenu => {
                todo!("dmenu not implemented")
            }
            Mode::File => {
                mode::file(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Math => {
                mode::math(&config).map_err(|e| anyhow!(e))?;
            }
            Mode::Auto => {
                mode::auto(&config)?;
            }
        }

        Ok(())
    } else {
        Err(anyhow!("No mode provided"))
    }
}
