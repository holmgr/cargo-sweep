extern crate chrono;
extern crate clap;
extern crate failure;
extern crate walkdir;
#[macro_use]
extern crate log;
extern crate cargo_metadata;
extern crate fern;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

use clap::{App, Arg, ArgGroup, SubCommand};
use fern::colors::{Color, ColoredLevelConfig};
use fingerprint::{
    remove_not_built_with,
    remove_older_then
};
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

mod fingerprint;
mod stamp;
mod util;
use self::stamp::Timestamp;
use self::util::format_bytes;

/// Setup logging according to verbose flag.
fn setup_logging(verbose: bool) {
    // configure colors for the whole line
    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::White)
        .debug(Color::White)
        .trace(Color::BrightBlack);

    let level = if verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let colors_level = colors_line.info(Color::Green);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{color_line}[{level}{color_line}] {message}\x1B[0m",
                color_line = format_args!(
                    "\x1B[{}m",
                    colors_line.get_color(&record.level()).to_fg_str()
                ),
                level = colors_level.color(record.level()),
                message = message,
            ));
        })
        .level(level)
        .level_for("pretty_colored", log::LevelFilter::Trace)
        .chain(std::io::stdout())
        .apply()
        .unwrap();
}

/// Returns whether the given path points to a valid Cargo project.
fn is_cargo_root(path: &Path) -> bool {
    let mut path = path.to_path_buf();

    // Check that cargo.toml exists.
    path.push("Cargo.toml");
    if let Ok(metadata) = cargo_metadata::metadata(Some(path.as_path())) {
        Path::new(&metadata.target_directory).exists()
    } else {
        false
    }
}

/// Find all cargo project under the given root path.
fn find_cargo_projects(root: &Path) -> Vec<PathBuf> {
    let mut project_paths = vec![];
    // Sub directories cannot be checked due to internal crates.
    for entry in WalkDir::new(root.to_str().unwrap())
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_dir() && is_cargo_root(entry.path()) {
                project_paths.push(entry.path().to_path_buf());
            }
        }
    }
    project_paths
}

#[allow(clippy::cyclomatic_complexity)]
fn main() {
    let matches = App::new("Cargo sweep")
        .version("0.1")
        .author("Viktor Holmgren <viktor.holmgren@gmail.com>")
        .about("Clean old/unused Cargo artifacts")
        .subcommand(
            SubCommand::with_name("sweep")
                .arg(
                    Arg::with_name("verbose")
                        .short("v")
                        .help("Turn verbose information on"),
                )
                .arg(
                    Arg::with_name("recursive")
                        .short("r")
                        .help("Apply on all projects below the given path"),
                )
                .arg(
                    Arg::with_name("dry-run")
                        .short("d")
                        .help("Dry run which will not delete any files"),
                )
                .arg(
                    Arg::with_name("stamp")
                        .short("s")
                        .long("stamp")
                        .help("Store timestamp file at the given path, is used by file option"),
                )
                .arg(
                    Arg::with_name("file")
                        .short("f")
                        .long("file")
                        .help("Load timestamp file in the given path, cleaning everything older"),
                )
                .arg(
                    Arg::with_name("installed")
                        .short("i")
                        .long("installed")
                        .help("Keep only artefacts made by Toolchains currently installed by rustup")
                )
                .arg(
                    Arg::with_name("toolchains")
                        .long("toolchains")
                        .value_name("toolchains")
                        .help("Toolchains (currently installed by rustup) that shuld have there artefacts kept.")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("time")
                        .short("t")
                        .long("time")
                        .value_name("days")
                        .help("Number of days backwards to keep. If no value is set uses 30.")
                        .takes_value(true),
                )
                .group(
                    ArgGroup::with_name("timestamp")
                        .args(&["stamp", "file", "time", "installed", "toolchains"])
                        .required(true),
                )
                .arg(
                    Arg::with_name("path")
                        .index(1)
                        .value_name("path")
                        .help("Path to check"),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("sweep") {
        let verbose = matches.is_present("verbose");
        setup_logging(verbose);

        let dry_run = matches.is_present("dry-run");

        // Default to current invocation path.
        let path = match matches.value_of("path") {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().expect("Failed to get current directory"),
        };

        let keep_duration = if matches.is_present("file") {
            let ts = Timestamp::load(path.as_path()).expect("Failed to load timestamp file");
            Duration::from(ts)
        } else {
            let days_to_keep: u64 = matches
                .value_of("time")
                .unwrap_or("30")
                .parse()
                .expect("Invalid time format");
            Duration::from_secs(days_to_keep * 24 * 3600)
        };

        if matches.is_present("stamp") {
            debug!("Writing timestamp file in: {:?}", path);
            match Timestamp::new().store(path.as_path()) {
                Ok(_) => {}
                Err(e) => error!("Failed to write timestamp file: {}", e),
            }
            return;
        }

        if matches.is_present("installed") || matches.is_present("toolchains") {
            if matches.is_present("recursive") {
                for project_path in find_cargo_projects(&path) {
                    match remove_not_built_with(
                        &project_path,
                        matches.value_of("toolchains"),
                        dry_run,
                    ) {
                        Ok(cleaned_amount) if dry_run => {
                            info!("Would clean: {}", format_bytes(cleaned_amount))
                        }
                        Ok(cleaned_amount) => info!("Cleaned {}", format_bytes(cleaned_amount)),
                        Err(e) => error!("Failed to clean {:?}: {}", path, e),
                    };
                }
            } else {
                match remove_not_built_with(&path, matches.value_of("toolchains"), dry_run) {
                    Ok(cleaned_amount) if dry_run => {
                        info!("Would clean: {}", format_bytes(cleaned_amount))
                    }
                    Ok(cleaned_amount) => info!("Cleaned {}", format_bytes(cleaned_amount)),
                    Err(e) => error!("Failed to clean {:?}: {}", path, e),
                };
            }
            return;
        }

        if matches.is_present("recursive") {
            for project_path in find_cargo_projects(&path) {
                match remove_older_then(&project_path, &keep_duration, dry_run) {
                    Ok(cleaned_amount) if dry_run => {
                        info!("Would clean: {}", format_bytes(cleaned_amount))
                    }
                    Ok(cleaned_amount) => info!("Cleaned {}", format_bytes(cleaned_amount)),
                    Err(e) => error!("Failed to clean {:?}: {}", path, e),
                };
            }
        } else {
            match remove_older_then(&path, &keep_duration, dry_run) {
                Ok(cleaned_amount) if dry_run => {
                    info!("Would clean: {}", format_bytes(cleaned_amount))
                }
                Ok(cleaned_amount) => info!("Cleaned {}", format_bytes(cleaned_amount)),
                Err(e) => error!("Failed to clean {:?}: {}", path, e),
            };
        }
    }
}
