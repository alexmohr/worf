use std::env;

use anyhow::anyhow;
use worf_lib::config::Mode;
use worf_lib::{Error, config, mode};
fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    if let Some(show) = &config.show() {
        let result = match show {
            Mode::Run => mode::run(&config),
            Mode::Drun => mode::d_run(&config),
            Mode::Dmenu => mode::dmenu(&config),
            Mode::File => mode::file(&config),
            Mode::Math => {
                mode::math(&config);
                Ok(())
            }
            Mode::Ssh => mode::ssh(&config),
            Mode::Emoji => mode::emoji(&config),
            Mode::Auto => mode::auto(&config),
        };

        if let Err(err) = result {
            if err == Error::NoSelection {
                log::info!("no selection made");
            } else {
                log::error!("Error occurred {err:?}");
                return Err(anyhow!("Error occurred {err:?}"));
            }
        }

        Ok(())
    } else {
        Err(anyhow!("No mode provided"))
    }
}
