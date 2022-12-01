#![allow(deprecated)]
use anyhow::{bail, Context, Error};
use log::{debug, info, warn};
use serde_derive::Deserialize;
use serde_json::from_str;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, remove_dir_all, remove_file, File},
    hash::{Hash, Hasher, SipHasher},
    io::prelude::*,
    path::Path,
    process::Command,
    time::Duration,
};
use walkdir::{DirEntry, WalkDir};

/// This has to match the way Cargo hashes a rustc version.
/// As such it is copied from Cargos code.
fn hash_u64<H: Hash>(hashable: &H) -> u64 {
    let mut hasher = SipHasher::new_with_keys(0, 0);
    hashable.hash(&mut hasher);
    hasher.finish()
}

/// This has to match the way Cargo stores a rustc version in a fingerprint file.
#[derive(Deserialize, Debug)]
struct Fingerprint {
    rustc: u64,
}

/// the files and folder tracked by fingerprint have the form `({prefix}-)?{name}-{16 char hex hash}(.{extension})?`
/// this returns `Some({hex hash})` if it is of that form and `None` otherwise.
fn hash_from_path_name(filename: &str) -> Option<&str> {
    // maybe just use regex
    let name = filename.split('.').next().unwrap();
    let hash = name.rsplit('-').next().unwrap();
    if hash.len() == name.len() {
        // we did not find a dash, it cant be a fingerprint matched file.
        return None;
    }
    if !hash.chars().all(|x| x.is_ascii_hexdigit()) {
        // we found a non hex char, it cant be a fingerprint matched file.
        return None;
    }
    if hash.len() != 16 {
        // the hash part is the wrong length.
        // It is not a fingerprint just a project with an unfortunate name.
        return None;
    }
    Some(hash)
}

impl Fingerprint {
    /// Attempts to load the the Fingerprint data for a given fingerprint directory.
    fn load(fingerprint_dir: &Path) -> Result<Self, Error> {
        for entry in fs::read_dir(fingerprint_dir)? {
            let path = entry?.path();
            if let Some(ext) = path.extension() {
                if ext == "json" {
                    let mut file = File::open(&path)?;
                    let mut contents = String::new();
                    file.read_to_string(&mut contents)?;
                    if let Ok(fing) = from_str(&contents) {
                        return Ok(fing);
                    }
                }
            }
        }
        bail!("did not fine a fingerprint file in {:?}", fingerprint_dir)
    }
}

fn load_all_fingerprints_built_with(
    fingerprint_dir: &Path,
    installed_rustc: &HashSet<u64>,
) -> Result<HashSet<String>, Error> {
    assert_eq!(
        fingerprint_dir
            .file_name()
            .expect("load takes the path to a .fingerprint directory"),
        ".fingerprint"
    );
    let mut keep = HashSet::new();
    for entry in fs::read_dir(fingerprint_dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let f = Fingerprint::load(&path).map(|f| installed_rustc.contains(&f.rustc));
            // we default to keeping, as there are files that dont have the data we need.
            if f.unwrap_or(true) {
                let name = path.file_name().unwrap().to_string_lossy();
                if let Some(hash) = hash_from_path_name(&name) {
                    keep.insert(hash.to_string());
                }
            }
        }
    }
    debug!("Hashs to keep: {:#?}", keep);
    Ok(keep)
}

fn last_used_time(fingerprint_dir: &Path) -> Result<Duration, Error> {
    let mut best = Duration::from_secs(3_155_760_000); // 100 years!
    for entry in fs::read_dir(fingerprint_dir)? {
        let accessed = entry?
            .metadata()?
            .accessed()?
            .elapsed()
            .unwrap_or(Duration::from_secs(0));
        if accessed < best {
            best = accessed;
        }
    }
    Ok(best)
}

fn load_all_fingerprints_by_time(fingerprint_dir: &Path) -> Result<Vec<(Duration, String)>, Error> {
    assert_eq!(
        fingerprint_dir
            .file_name()
            .expect("load takes the path to a .fingerprint directory"),
        ".fingerprint"
    );
    let mut keep = vec![];
    for entry in fs::read_dir(fingerprint_dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let last_used = last_used_time(&path)?;
            let name = path.file_name().unwrap().to_string_lossy();
            if let Some(hash) = hash_from_path_name(&name) {
                keep.push((last_used, hash.to_string()));
            }
        }
    }
    keep.sort_unstable();
    debug!("Hashs by time: {:#?}", keep);
    Ok(keep)
}

fn load_all_fingerprints_newer_than(
    fingerprint_dir: &Path,
    keep_duration: &Duration,
) -> Result<HashSet<String>, Error> {
    assert_eq!(
        fingerprint_dir
            .file_name()
            .expect("load takes the path to a .fingerprint directory"),
        ".fingerprint"
    );
    let mut keep = HashSet::new();
    for entry in fs::read_dir(fingerprint_dir)? {
        let path = entry?.path();
        if path.is_dir() && last_used_time(&path)? < *keep_duration {
            let name = path.file_name().unwrap().to_string_lossy();
            if let Some(hash) = hash_from_path_name(&name) {
                keep.insert(hash.to_string());
            }
        }
    }
    debug!("Hashs to keep: {:#?}", keep);
    Ok(keep)
}

fn total_disk_space_dir(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .fold(0, |acc, m| acc + m.len())
}

fn total_disk_space_by_hash_in_a_dir(
    dir: &Path,
    disk_space: &mut HashMap<String, u64>,
) -> Result<(), Error> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let path = entry.path();
        let name = path
            .file_name()
            .expect("folders in a directory don't have a name!?")
            .to_string_lossy();

        if let Some(hash) = hash_from_path_name(&name) {
            *disk_space.entry(hash.to_owned()).or_default() += if path.is_file() {
                metadata.len()
            } else if path.is_dir() {
                total_disk_space_dir(&path)
            } else {
                panic!("what type is it!")
            };
        }
    }
    Ok(())
}

fn remove_not_matching_in_a_dir(
    dir: &Path,
    keep: &HashSet<String>,
    dry_run: bool,
) -> Result<u64, Error> {
    let mut total_disk_space = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let path = entry.path();
        let name = path
            .file_name()
            .expect("folders in a directory don't have a name!?")
            .to_string_lossy();
        if let Some(hash) = hash_from_path_name(&name) {
            if !keep.contains(hash) {
                if path.is_file() {
                    total_disk_space += metadata.len();
                    if !dry_run {
                        match remove_file(&path) {
                            Ok(_) => info!("Successfully removed: {:?}", &path),
                            Err(e) => warn!("Failed to remove: {:?} {}", &path, e),
                        };
                    } else {
                        info!("Would remove: {:?}", &path);
                    }
                } else if path.is_dir() {
                    total_disk_space += total_disk_space_dir(&path);
                    if !dry_run {
                        match remove_dir_all(&path) {
                            Ok(_) => info!("Successfully removed: {:?}", &path),
                            Err(e) => warn!("Failed to remove: {:?} {}", &path, e),
                        };
                    } else {
                        info!("Would remove: {:?}", &path);
                    }
                }
            }
        }
    }
    Ok(total_disk_space)
}

fn total_disk_space_in_a_profile(dir: &Path) -> Result<HashMap<String, u64>, Error> {
    debug!("Sizing: {:?} with total_disk_space_in_a_profile", dir);
    let mut total_disk_space = HashMap::new();
    total_disk_space_by_hash_in_a_dir(&dir.join(".fingerprint"), &mut total_disk_space)?;
    total_disk_space_by_hash_in_a_dir(&dir.join("build"), &mut total_disk_space)?;
    total_disk_space_by_hash_in_a_dir(&dir.join("deps"), &mut total_disk_space)?;
    // examples is just final artifacts not tracked by fingerprint so skip that one.
    // incremental is not tracked by fingerprint so skip that one.
    // `native` isn't generated by cargo since 1.37.0
    let native_dir = dir.join("native");
    if native_dir.exists() {
        total_disk_space_by_hash_in_a_dir(&native_dir, &mut total_disk_space)?;
    }
    total_disk_space_by_hash_in_a_dir(dir, &mut total_disk_space)?;
    Ok(total_disk_space)
}

fn remove_not_built_with_in_a_profile(
    dir: &Path,
    keep: &HashSet<String>,
    dry_run: bool,
) -> Result<u64, Error> {
    debug!(
        "cleaning: {:?} with remove_not_built_with_in_a_profile",
        dir
    );
    let mut total_disk_space = 0;
    total_disk_space += remove_not_matching_in_a_dir(&dir.join("build"), keep, dry_run)?;
    total_disk_space += remove_not_matching_in_a_dir(&dir.join("deps"), keep, dry_run)?;
    // examples is just final artifacts not tracked by fingerprint so skip that one.
    // incremental is not tracked by fingerprint so skip that one.
    // `native` isn't generated by cargo since 1.37.0
    let native_dir = dir.join("native");
    if native_dir.exists() {
        total_disk_space += remove_not_matching_in_a_dir(&native_dir, keep, dry_run)?;
    }
    total_disk_space += remove_not_matching_in_a_dir(dir, keep, dry_run)?;
    total_disk_space += remove_not_matching_in_a_dir(&dir.join(".fingerprint"), keep, dry_run)?;
    Ok(total_disk_space)
}

fn lookup_all_fingerprint_dirs(dir: &Path) -> impl Iterator<Item = DirEntry> {
    WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|s| s == ".fingerprint")
                .unwrap_or(false)
        })
}

fn lookup_from_names(
    iter: impl Iterator<Item = Option<impl AsRef<str>>>,
) -> Result<HashSet<u64>, Error> {
    iter.map(|x| {
        let args = x
            .into_iter()
            .map(|toolchain| format!("+{}", toolchain.as_ref()))
            .chain(Some("-vV".to_string()));
        let out = Command::new("rustc")
            .args(args)
            .output()
            .context("failed to run `rustc`")?;

        if !out.status.success() {
            if !out.stdout.is_empty() {
                bail!(String::from_utf8_lossy(&out.stdout).to_string());
            } else {
                bail!(String::from_utf8_lossy(&out.stderr).to_string());
            }
        }
        Ok(hash_u64(&String::from_utf8_lossy(&out.stdout)))
    })
    .chain(
        // Some fingerprints made to track the output of build scripts claim to have been built with a rust that hashes to 0.
        // This can be fixed in cargo, but for now this makes sure we don't clean the files.
        Some(Ok(0)),
    )
    .collect()
}

fn rustup_toolchain_list() -> Option<Vec<String>> {
    let out = Command::new("rustup").args(["toolchain", "list"]).output();

    match out {
        Ok(out) if out.status.success() => {
            let res = String::from_utf8_lossy(&out.stdout)
                .split('\n')
                .filter_map(|x| x.split_whitespace().next())
                .map(|x| x.trim().to_owned())
                .collect::<Vec<String>>();

            Some(res)
        }

        // Ouch, rustup was not available or something.
        // Let's just fallback to the bare `rustc` and hope for the best.
        _ => None,
    }
}

pub fn remove_not_built_with(
    dir: &Path,
    rust_version_to_keep: Option<&str>,
    dry_run: bool,
) -> Result<u64, Error> {
    debug!("cleaning: {:?} with remove_not_built_with", dir);
    let mut total_disk_space = 0;
    let hashed_rust_version_to_keep = if let Some(names) = rust_version_to_keep {
        info!(
            "Using specified installed toolchains: {:?}",
            names.split(',').collect::<Vec<_>>()
        );
        lookup_from_names(names.split(',').map(Some))?
    } else {
        match rustup_toolchain_list() {
            Some(list) => {
                info!("Using all installed toolchains: {:?}", list);
                lookup_from_names(list.iter().map(Some))?
            }

            None => {
                info!("Couldn't identify the installed toolchains, using bare `rustc` call");
                let list: Vec<Option<String>> = vec![None];
                lookup_from_names(list.into_iter())?
            }
        }
    };
    for fing in lookup_all_fingerprint_dirs(dir) {
        let path = fing.into_path();
        let keep = load_all_fingerprints_built_with(&path, &hashed_rust_version_to_keep)?;
        total_disk_space +=
            remove_not_built_with_in_a_profile(path.parent().unwrap(), &keep, dry_run)?;
    }
    Ok(total_disk_space)
}

/// Attempts to sweep the cargo project located at the given path,
/// keeping only files which have been accessed within the given duration.
/// Dry specifies if files should actually be removed or not.
/// Returns a list of the deleted file/dir paths.
pub fn remove_older_than(
    path: &Path,
    keep_duration: &Duration,
    dry_run: bool,
) -> Result<u64, Error> {
    debug!("cleaning: {:?} with remove_older_than", path);
    let mut total_disk_space = 0;

    for fing in lookup_all_fingerprint_dirs(path) {
        let path = fing.into_path();
        let keep = load_all_fingerprints_newer_than(&path, keep_duration)?;
        total_disk_space +=
            remove_not_built_with_in_a_profile(path.parent().unwrap(), &keep, dry_run)?;
    }

    Ok(total_disk_space)
}

pub fn remove_older_until_fits(path: &Path, target_size: u64, dry_run: bool) -> Result<u64, Error> {
    debug!("cleaning: {:?} with remove_older_until_fits", path);
    let starting_size = total_disk_space_dir(path);
    if starting_size <= target_size {
        // already below target
        return Ok(0);
    }
    let size_to_remove = starting_size - target_size;
    debug!("size_to_remove: {:?}", size_to_remove);

    let fingerprint_dirs: Vec<DirEntry> = lookup_all_fingerprint_dirs(path).collect();
    let mut order: Vec<(Duration, u64, &Path, String)> = vec![];
    for fing in &fingerprint_dirs {
        let path = fing.path();
        let sizes = total_disk_space_in_a_profile(path.parent().unwrap())?;
        for (last_used, hash) in load_all_fingerprints_by_time(path)? {
            order.push((
                last_used,
                *(sizes.get(&hash).unwrap_or(&0)),
                fing.path(),
                hash,
            ));
        }
    }

    // as Duration is first in the elements of order this sorts items from new to old
    order.sort();

    let mut removed = 0u64;
    // organized keeps track of what needs to be keep per fingerprint dirs
    let mut organized = HashMap::new();
    let mut printed = false;

    for dir in &fingerprint_dirs {
        // populate organized with keeping nothing in each fingerprint dirs
        organized.entry(dir.path()).or_insert_with(HashSet::new);
    }

    for (last_used, sizes, fing, hash) in order.into_iter().rev() {
        if removed + sizes < size_to_remove {
            removed += sizes;
            continue;
        }
        if !printed {
            // TODO: consider formatting better for printing
            info!("Removing older then: {:?}", &last_used);
            printed = true;
        }
        organized
            .entry(fing)
            .or_insert_with(HashSet::new)
            .insert(hash);
    }

    let mut total_disk_space = 0;

    for (fing, keep) in organized {
        total_disk_space +=
            remove_not_built_with_in_a_profile(fing.parent().unwrap(), &keep, dry_run)?;
    }

    Ok(total_disk_space)
}
