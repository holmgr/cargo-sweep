use anyhow::Context;
use cargo_metadata::{Error, Metadata, MetadataCommand};
use crossterm::tty::IsTty;
use fern::colors::{Color, ColoredLevelConfig};
use log::{debug, error, info};
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

mod cli;
mod fingerprint;
mod stamp;
mod util;

use self::fingerprint::{remove_not_built_with, remove_older_than, remove_older_until_fits};
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

    let isatty = std::io::stdout().is_tty();

    let colors_level = colors_line.info(Color::Green);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            if isatty {
                out.finish(format_args!(
                    "{color_line}[{level}{color_line}] {message}\x1B[0m",
                    color_line = format_args!(
                        "\x1B[{}m",
                        colors_line.get_color(&record.level()).to_fg_str()
                    ),
                    level = colors_level.color(record.level()),
                    message = message,
                ))
            } else {
                out.finish(format_args!("[{}] {message}", record.level()))
            }
        })
        .level(level)
        .level_for("pretty_colored", log::LevelFilter::Trace)
        .chain(std::io::stdout())
        .apply()
        .unwrap();
}

/// Returns whether the given path to a Cargo.toml points to a real target directory.
fn is_cargo_root(path: &Path) -> Option<PathBuf> {
    if let Ok(metadata) = metadata(path) {
        let out = Path::new(&metadata.target_directory).to_path_buf();
        if out.exists() {
            return Some(out);
        }
    }
    None
}

/// is a `DirEntry` a unix stile hidden file, ie starts with `.`
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// Find all cargo project under the given root path.
fn find_cargo_projects(root: &Path, include_hidden: bool) -> Vec<PathBuf> {
    let mut target_paths = std::collections::BTreeSet::new();

    let mut iter = WalkDir::new(root).min_depth(1).into_iter();

    while let Some(entry) = iter.next() {
        if let Ok(entry) = entry {
            if entry.file_type().is_dir() {
                if !include_hidden && is_hidden(&entry) {
                    debug!("skip hidden folder {}", entry.path().display());
                    iter.skip_current_dir();
                    continue;
                }
                if entry.path().ancestors().any(|a| target_paths.contains(a)) {
                    // no reason to look at the contents of something we are already cleaning.
                    // Yes ancestors is a inefficient way to check. We can use a trie or something if it is slow.
                    iter.skip_current_dir();
                    continue;
                }
            }
            if entry.file_name() != "Cargo.toml" {
                continue;
            }
            if let Some(target_directory) = is_cargo_root(entry.path()) {
                target_paths.insert(target_directory);
                iter.skip_current_dir(); // no reason to look at the src and such
            }
        }
    }
    target_paths.into_iter().collect()
}

fn metadata(path: &Path) -> Result<Metadata, Error> {
    let manifest_path = if path.file_name().and_then(OsStr::to_str) == Some("Cargo.toml") {
        path.to_owned()
    } else {
        path.join("Cargo.toml")
    };

    MetadataCommand::new()
        .manifest_path(manifest_path)
        .no_deps()
        .exec()
}

fn main() -> anyhow::Result<()> {
    let matches = cli::arg_matches();

    if let Some(matches) = matches.subcommand_matches("sweep") {
        let verbose = matches.is_present("verbose");
        setup_logging(verbose);

        let dry_run = matches.is_present("dry-run");

        // Default to current invocation path.
        let path = match matches.value_of("path") {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().expect("Failed to get current directory"),
        };

        if matches.is_present("stamp") {
            debug!("Writing timestamp file in: {:?}", path);
            return Timestamp::new()
                .store(path.as_path())
                .context("Failed to write timestamp file");
        }

        let paths = if matches.is_present("recursive") {
            find_cargo_projects(&path, matches.is_present("hidden"))
        } else {
            let metadata = metadata(&path).context(format!(
                "Failed to gather metadata for {:?}",
                path.display()
            ))?;
            let out = Path::new(&metadata.target_directory).to_path_buf();
            if out.exists() {
                vec![out]
            } else {
                anyhow::bail!("Failed to clean {:?} as it does not exist.", out);
            }
        };

        if matches.is_present("installed") || matches.is_present("toolchains") {
            for project_path in &paths {
                match remove_not_built_with(project_path, matches.value_of("toolchains"), dry_run) {
                    Ok(cleaned_amount) if dry_run => {
                        info!(
                            "Would clean: {} from {project_path:?}",
                            format_bytes(cleaned_amount)
                        )
                    }
                    Ok(cleaned_amount) => info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes(cleaned_amount)
                    ),
                    Err(e) => error!(
                        "{:?}",
                        e.context(format!("Failed to clean {project_path:?}"))
                    ),
                };
            }
        } else if matches.is_present("maxsize") {
            // TODO: consider parsing units like GB, KB ...
            let size = match matches
                .value_of("maxsize")
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(s) => s * 1024 * 1024,
                None => {
                    anyhow::bail!("maxsize has to be a number");
                }
            };

            for project_path in &paths {
                match remove_older_until_fits(project_path, size, dry_run) {
                    Ok(cleaned_amount) if dry_run => {
                        info!(
                            "Would clean: {} from {project_path:?}",
                            format_bytes(cleaned_amount)
                        )
                    }
                    Ok(cleaned_amount) => info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes(cleaned_amount)
                    ),
                    Err(e) => error!("Failed to clean {:?}: {:?}", project_path, e),
                };
            }
        } else {
            let keep_duration = if matches.is_present("file") {
                let ts = Timestamp::load(path.as_path()).expect("Failed to load timestamp file");
                Duration::from(ts)
            } else {
                let days_to_keep: u64 = matches
                    .value_of("time")
                    .expect("--time argument missing")
                    .parse()
                    .expect("Invalid time format");
                Duration::from_secs(days_to_keep * 24 * 3600)
            };

            for project_path in &paths {
                match remove_older_than(project_path, &keep_duration, dry_run) {
                    Ok(cleaned_amount) if dry_run => {
                        info!(
                            "Would clean: {} from {project_path:?}",
                            format_bytes(cleaned_amount)
                        )
                    }
                    Ok(cleaned_amount) => info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes(cleaned_amount)
                    ),
                    Err(e) => error!("Failed to clean {:?}: {:?}", project_path, e),
                };
            }
        }
    }

    Ok(())
}
