extern crate clap;
extern crate failure;
extern crate walkdir;

use clap::{App, Arg, SubCommand};
use failure::Error;
use std::{
    env,
    fs::{read_dir, remove_dir, remove_file},
    path::{Path, PathBuf},
    time::Duration,
};
use walkdir::WalkDir;

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
        println!(
            "{:?} Access time: {:?} comp: {:?}",
            entry.path(),
            access_time.elapsed()?,
            keep_duration
        );
        if access_time.elapsed()? > *keep_duration {
            cleaned_file_paths.push(entry.path().to_path_buf());

            // Remove only empty directories.
            if metadata.file_type().is_dir() && read_dir(entry.path())?.count() == 0 {
                remove_dir(entry.path())?;
            } else if metadata.file_type().is_file() {
                remove_file(entry.path())?;
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

        // Default to current invocation path.
        let path = match matches.value_of("path") {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().expect("Failed to get current directory"),
        };

        match try_clean_path(&path, &keep_duration) {
            Ok(paths) => println!("Cleaned paths: \n {:#?}", paths),
            Err(e) => eprintln!("Failed to clean {:?}: {}", path, e),
        };
    }
}
