use std::env;

use anyhow::anyhow;

use worf::{Error, config, config::Mode, desktop::fork_if_configured, modes};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .parse_filters(&env::var("RUST_LOG").unwrap_or_else(|_| "error".to_owned()))
        .format_timestamp_micros()
        .init();

    let args = config::parse_args();

    let config = config::load_config(Some(&args));
    let config = match config {
        Ok(c) => c,
        Err(e) => {
            log::error!("error during config load, skipping it, {e}");
            args
        }
    };

    if config.version() {
        println!("worf version {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    fork_if_configured(&config); // may exit the program

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
            Mode::WebSearch => modes::search::show(&config),
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
