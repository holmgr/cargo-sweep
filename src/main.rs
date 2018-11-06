extern crate clap;
extern crate serde_json;
extern crate walkdir;

use clap::{App, Arg, SubCommand};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    str::from_utf8,
};
use walkdir::WalkDir;

fn remove_file_dir(file: &Path) {
    if fs::metadata(&file).unwrap().is_dir() {
        fs::remove_dir_all(file).expect("Failed to remove file");
    } else {
        fs::remove_file(file).expect("Failed to remove file");
    }
}

/// Returns the hash contained in the filename of the path, if it exists.
fn get_hash(path: PathBuf) -> Option<String> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name_str| {
            let (_, hash) = name_str.split_at(name_str.find("-").unwrap_or(0) + 1);
            String::from(hash)
        })
        // Only keep real hashes, i.e 16 length caracter strings.
        // TODO: Do something more sophisticated.
        .and_then(|hash| if hash.len() == 16 { Some(hash) } else { None })
}

fn main() {
    let matches = App::new("Cargo sweep")
        .version("0.1")
        .author("Viktor Holmgren <viktor.holmgren@gmail.com>")
        .about("Clean old/unused Cargo artifacts")
        .subcommand(
            SubCommand::with_name("sweep")
                .about("Sweeps the target directory")
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
        let days_to_keep: u32 = matches
            .value_of("time")
            .unwrap()
            .parse()
            .expect("Invalid time format");

        let path = match matches.value_of("path") {
            Some(p) => PathBuf::from(p),
            None => env::current_dir().expect("Failed to get current directory"),
        };

        println!("Days to keep: {}", days_to_keep);
        println!("Path to clean: {:?}", path);

        let check_cmd_output = Command::new("cargo")
            .args(&["build", "--message-format=json"])
            .current_dir(&path)
            .output()
            .expect("Failed to run cargo check")
            .stdout;
        let cmd_str = from_utf8(&check_cmd_output).unwrap();

        // TODO: Extract hashes instead, find all files containing them and delete.
        let filenames = cmd_str
            .lines()
            .flat_map(|line| {
                let check_json: Value = serde_json::from_str(line).expect("Failed to parse json");
                match check_json["filenames"] {
                    Value::Array(ref fs) => fs
                        .iter()
                        .cloned()
                        .filter_map(|f| match f {
                            Value::String(s) => Some(s),
                            _ => None,
                        }).collect::<Vec<_>>(),
                    _ => vec![],
                }.into_iter()
            }).collect::<Vec<String>>();

        let hashes: Vec<_> = filenames
            .into_iter()
            .filter_map(|f| get_hash(PathBuf::from(f)))
            .collect();

        println!("Found hashes\n {:#?}", hashes);

        let mut target_path = path.clone();
        target_path.push("target");
        for entry in WalkDir::new(target_path.to_str().unwrap())
            .contents_first(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.into_path();
            if let Some(entry_hash) = get_hash(entry_path.clone()) {
                if hashes.iter().any(|h| *h == entry_hash) {
                    remove_file_dir(&entry_path);
                }
            }
        }
    }
}
