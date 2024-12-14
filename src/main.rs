use anyhow::Context;
use cargo_metadata::{Error, Metadata, MetadataCommand};
use crossterm::tty::IsTty;
use fern::colors::{Color, ColoredLevelConfig};

use log::{debug, error, info, warn};
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

use self::cli::Criterion;
use self::fingerprint::{
    hash_toolchains, remove_not_built_with, remove_older_than, remove_older_until_fits,
};
use self::stamp::Timestamp;
use self::util::{format_bytes, format_bytes_or_nothing};

/// Setup logging according to verbose flag.
fn setup_logging(verbosity_level: u8) {
    let level = match verbosity_level {
        0 => log::LevelFilter::Info,
        1 => log::LevelFilter::Debug,
        2.. => log::LevelFilter::Trace,
    };

    let isatty = std::io::stdout().is_tty();

    // Configure colors for each log line
    let level_colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .error(Color::Red)
        .warn(Color::Yellow)
        .debug(Color::BrightBlack);

    fern::Dispatch::new()
        .format(move |out, message, record| {
            if isatty {
                let reset = "\x1B[0m";
                let level_color = level_colors.color(record.level());
                out.finish(format_args!("{reset}[{level_color}{reset}] {message}"));
            } else {
                out.finish(format_args!("[{}] {message}", record.level()));
            }
        })
        .level(level)
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
        .map_or(false, |s| s.starts_with('.'))
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
                // Previously cargo-sweep skipped subdirectories here, but it is valid for
                // subdirectories to contain cargo roots.
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
    let args = cli::parse();

    let criterion = args.criterion()?;
    let dry_run = args.dry_run;
    setup_logging(args.verbose);

    // Default to current invocation path.
    let paths = match args.path.len() {
        0 => vec![env::current_dir().expect("Failed to get current directory")],
        _ => args.path,
    };

    // FIXME: Change to write to every passed in path instead of just the first one
    if let Criterion::Stamp = criterion {
        if paths.len() > 1 {
            anyhow::bail!("Using multiple paths and --stamp is currently unsupported");
        }

        debug!("Writing timestamp file in: {:?}", paths[0]);
        return Timestamp::new()
            .store(paths[0].as_path())
            .context("Failed to write timestamp file");
    };

    let processed_paths = if args.recursive {
        info!("Searching recursively for Rust project folders");
        paths
            .iter()
            .flat_map(|path| find_cargo_projects(path, args.hidden))
            .collect::<Vec<_>>()
    } else {
        let mut return_paths = Vec::with_capacity(paths.len());
        for path in &paths {
            let metadata = metadata(path).context(format!(
                "Failed to gather metadata for {:?}",
                path.display()
            ))?;
            let out = Path::new(&metadata.target_directory).to_path_buf();
            if out.exists() {
                return_paths.push(out);
            } else {
                warn!("Failed to clean {:?} as it does not exist.", out)
            };
        }
        return_paths
    };

    let mut total_cleaned = 0;

    // `None`: do not remove based on toolchain version
    // `Some(None)`: remove all installed toolchains
    // `Some(Some(Vec))`: remove only the specified toolchains
    let toolchains = match &criterion {
        Criterion::Installed => Some(None),
        Criterion::Toolchains(vec) => Some(Some(vec.clone())),
        _ => None,
    };
    if let Some(toolchains) = toolchains {
        let hashed_toolchains = match hash_toolchains(toolchains.as_ref()) {
            Ok(toolchains) => toolchains,
            Err(err) => {
                error!("{:?}", err.context("Failed to load toolchains."));
                return Ok(());
            }
        };

        for project_path in &processed_paths {
            match remove_not_built_with(project_path, &hashed_toolchains, dry_run) {
                Ok(cleaned_amount) if dry_run => {
                    info!(
                        "Would clean: {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Ok(cleaned_amount) => {
                    info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Err(e) => error!(
                    "{:?}",
                    e.context(format!("Failed to clean {project_path:?}"))
                ),
            };
        }
    } else if let Criterion::MaxSize(size) = criterion {
        for project_path in &processed_paths {
            match remove_older_until_fits(project_path, size, dry_run) {
                Ok(cleaned_amount) if dry_run => {
                    info!(
                        "Would clean: {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Ok(cleaned_amount) => {
                    info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Err(e) => error!("Failed to clean {:?}: {:?}", project_path, e),
            };
        }
    } else {
        let keep_duration = if let Criterion::File = criterion {
            let ts = Timestamp::load(paths[0].as_path(), dry_run)?;
            Duration::from(ts)
        } else if let Criterion::Time(days_to_keep) = criterion {
            Duration::from_secs(days_to_keep * 24 * 3600)
        } else {
            unreachable!("unknown criteria {:?}", criterion);
        };

        for project_path in &processed_paths {
            match remove_older_than(project_path, &keep_duration, dry_run) {
                Ok(cleaned_amount) if dry_run => {
                    info!(
                        "Would clean: {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Ok(cleaned_amount) => {
                    info!(
                        "Cleaned {} from {project_path:?}",
                        format_bytes_or_nothing(cleaned_amount)
                    );
                    total_cleaned += cleaned_amount;
                }
                Err(e) => error!("Failed to clean {:?}: {:?}", project_path, e),
            };
        }

        if processed_paths.len() > 1 {
            info!("Total amount: {}", format_bytes(total_cleaned));
        }
    }

    Ok(())
}
