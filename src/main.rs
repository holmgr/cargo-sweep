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

use self::cli::Criterion;
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

    let criterion = args.criterion();
    let dry_run = args.dry_run;
    setup_logging(args.verbose);

    // Default to current invocation path.
    let path = args
        .path
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current directory"));

    if let Criterion::Stamp = criterion {
        debug!("Writing timestamp file in: {:?}", path);
        return Timestamp::new()
            .store(path.as_path())
            .context("Failed to write timestamp file");
    }

    let paths = if args.recursive {
        find_cargo_projects(&path, args.hidden)
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

    let toolchains = match &criterion {
        Criterion::Installed => Some(vec![]),
        Criterion::Toolchains(vec) => Some(vec).cloned(),
        _ => None,
    };
    if let Some(toolchains) = toolchains {
        for project_path in &paths {
            match remove_not_built_with(project_path, &toolchains, dry_run) {
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
    } else if let Criterion::MaxSize(size) = criterion {
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
        let keep_duration = if let Criterion::File = criterion {
            let ts =
                Timestamp::load(path.as_path(), dry_run).expect("Failed to load timestamp file");
            Duration::from(ts)
        } else if let Criterion::Time(days_to_keep) = criterion {
            Duration::from_secs(days_to_keep * 24 * 3600)
        } else {
            unreachable!();
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

    Ok(())
}
