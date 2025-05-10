use std::env;

use anyhow::anyhow;
use worf_lib::config::Mode;
use worf_lib::{Error, config, modes};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();
    let config = config::load_config(Some(&args)).unwrap_or(args);

    if let Some(show) = &config.show() {
        let result = match show {
            Mode::Run => modes::run::show(&config),
            Mode::Drun => modes::drun::show(&config),
            Mode::Dmenu => modes::dmenu::show(&config),
            Mode::File => modes::file::show(&config),
            Mode::Math => {
                modes::math::show(&config);
                Ok(())
            }
            Mode::Ssh => modes::ssh::show(&config),
            Mode::Emoji => modes::emoji::show(&config),
            Mode::Auto => modes::auto::show(&config),
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
        log::error!("No mode provided");
        Ok(())
    }
}
