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
                mode::d_run(&mut config)?;
            }
            Mode::Dmenu => {
                todo!("dmenu not implemented")
            }
            Mode::Auto => {
                mode::auto(&mut config)?;
            }
        }

        Ok(())
    } else {
        Err(anyhow!("No mode provided"))
    }
}

//
// fn main() -> anyhow::Result<()> {
//     env_logger::Builder::new()
//         // todo change to info as default
//         .parse_filters(&std::env::var("RUST_LOG").unwrap_or_else(|_| "debug".to_owned()))
//         .init();
//     let args = Args::parse();
//
//     let home_dir = std::env::var("HOME")?;
//     let config_path = args.config.as_ref().map(|c| PathBuf::from(c)).unwrap_or_else(||{
//         std::env::var("XDG_CONF_HOME")
//             .map_or(
//                 PathBuf::from(home_dir.clone()).join(".config"),
//                 |xdg_conf_home| PathBuf::from(&xdg_conf_home),
//             )
//             .join("wofi")// todo change to ravi
//             .join("config")
//     });
//
//     let colors_dir = std::env::var("XDG_CACHE_HOME")
//         .map_or(
//             PathBuf::from(home_dir.clone()).join(".cache"),
//             |xdg_conf_home| PathBuf::from(&xdg_conf_home),
//         )
//         .join("wal")
//         .join("colors");
//
//     let toml_content = fs::read_to_string(config_path)?;
//     let config: Config = toml::from_str(&toml_content).unwrap_or_default();
//
//
//
//     gtk4::init()?;
//
//     let application = Application::builder()
//         .application_id("com.example.FirstGtkApp")
//         .build();
//
//     application.connect_activate(|app| {
//         let window = ApplicationWindow::builder()
//             .application(app)
//             .title("First GTK Program")
//             .name("window")
//             .default_width(config.x.clone().unwrap())
//             .default_height(config.y.clone().unwrap())
//             .resizable(false)
//             .decorated(false)
//             .build();
//
//
//
//         // Create a dialog window
//         let dialog = Dialog::new();
//         dialog.set_title(Some("Custom Dialog"));
//         dialog.set_default_size(300, 150);
//
//         // Create a vertical box container for the dialog content
//         let mut vbox =gtk4:: Box::new(Orientation::Horizontal, 10);
//
//         // Add a label to the dialog
//         let label = Label::new(Some("This is a custom dialog!"));
//         vbox.append(&label);
//
//         // Set the dialog content
//         dialog.set_child(Some(&vbox));
//
//         // Show the dialog
//         dialog.present();
//     });
//
//     let empty_array: [&str; 0] = [];;
//
//
//     application.run_with_args(&empty_array);
//
//     debug!("merged config result {:#?}", config);
//
//
//     Ok(())
// }
