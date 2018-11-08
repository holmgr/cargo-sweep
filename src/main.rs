extern crate chrono;
extern crate clap;
extern crate failure;
extern crate walkdir;
#[macro_use]
extern crate log;
extern crate fern;

use clap::{App, Arg, SubCommand};
use failure::Error;
use fern::colors::{Color, ColoredLevelConfig};
use std::{
    env,
    fs::{read_dir, remove_dir, remove_file},
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

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
        log::LevelFilter::Warn
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
        }).level(level)
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
    if !path.as_path().exists() {
        return false;
    }

    // Check that target dir exists.
    path.pop();
    path.push("target/");
    path.as_path().exists()
}

/// Find all cargo project under the given root path.
fn find_cargo_projects(root: &Path) -> Vec<PathBuf> {
    let mut project_paths = vec![];
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

/// Attempts to sweep the cargo project lookated at the given path,
/// keeping only files which have been accessed within the given duration.
/// Returns a list of the deleted file/dir paths.
fn try_clean_path<'a>(path: &'a Path, keep_duration: &Duration) -> Result<Vec<PathBuf>, Error> {
    let mut cleaned_file_paths = vec![];

    let mut target_path = path.to_path_buf();
    target_path.push("target/");
    for entry in WalkDir::new(target_path.to_str().unwrap())
        .min_depth(1)
        .contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let metadata = entry.metadata()?;
        let access_time = metadata.accessed()?;
        if access_time.elapsed()? > *keep_duration {
            cleaned_file_paths.push(entry.path().to_path_buf());

            // Remove only empty directories.
            if metadata.file_type().is_dir() && read_dir(entry.path())?.count() == 0 {
                match remove_dir(entry.path()) {
                    Ok(_) => info!("Successfuly removed: {:?}", entry.path()),
                    Err(e) => warn!("Failed to remove: {:?} {}", entry.path(), e),
                };
            } else if metadata.file_type().is_file() {
                match remove_file(entry.path()) {
                    Ok(_) => info!("Successfuly removed: {:?}", entry.path()),
                    Err(e) => warn!("Failed to remove: {:?} {}", entry.path(), e),
                };
            }
        }
    }

    Ok(cleaned_file_paths)
}

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
                ).arg(
                    Arg::with_name("recursive")
                        .short("r")
                        .help("Apply on all projects below the given path"),
                ).arg(
                    Arg::with_name("time")
                        .short("t")
                        .long("time")
                        .value_name("days")
                        .help("Number of days to backwards to keep")
                        .takes_value(true)
                        .required(true),
                ).arg(
                    Arg::with_name("path")
                        .index(1)
                        .value_name("path")
                        .help("Path to check"),
                ),
        ).get_matches();

    if let Some(matches) = matches.subcommand_matches("sweep") {
        // First unwrap is safe due to clap check.
        let days_to_keep: u64 = matches
            .value_of("time")
            .unwrap()
            .parse()
            .expect("Invalid time format");
        let keep_duration = Duration::from_secs(days_to_keep * 24 * 3600);

        let verbose = matches.is_present("verbose");
        setup_logging(verbose);

        // Default to current invocation path.
        let path = match matches.value_of("path") {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().expect("Failed to get current directory"),
        };
        
        if matches.is_present("recursive") {
            for project_path in find_cargo_projects(&path) {
                match try_clean_path(&project_path, &keep_duration) {
                    Ok(_) => {}
                    Err(e) => error!("Failed to clean {:?}: {}", path, e),
                };
            }
        }
        else {
            match try_clean_path(&path, &keep_duration) {
                Ok(_) => {}
                Err(e) => error!("Failed to clean {:?}: {}", path, e),
            };
        }
    }
}
