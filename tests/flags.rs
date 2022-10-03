use std::{
    borrow::BorrowMut,
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use assert_cmd::{assert::Assert, cargo::cargo_bin};
use fs_extra::dir::get_size;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::{tempdir, TempDir};

struct AnyhowWithContext(anyhow::Error);
impl Debug for AnyhowWithContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", self.0)
    }
}
impl<T: Into<anyhow::Error>> From<T> for AnyhowWithContext {
    fn from(err: T) -> Self {
        Self(err.into())
    }
}

fn project_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("sample-project")
}
fn cargo() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.current_dir(project_dir());
    cmd
}

fn sweep(args: &[&str]) -> Command {
    let mut cmd = Command::new(cargo_bin("cargo-sweep"));
    cmd.arg("sweep").current_dir(project_dir()).args(args);
    cmd
}

/// Returns the size of the build directory.
fn build() -> Result<(u64, TempDir)> {
    let target = tempdir()?;
    let old_size = get_size(target.path())?;
    cargo()
        .arg("build")
        .env("CARGO_TARGET_DIR", target.path())
        .assert()
        .success();
    let new_size = get_size(target.path())?;
    assert!(new_size > old_size, "cargo didn't build anything");
    Ok((new_size, target))
}

fn clean_and_parse(target: &TempDir, args: &[&str]) -> Result<u64> {
    let dry_run = args.iter().any(|&f| f == "-d" || f == "--dry-run");

    let (remove_msg, clean_msg) = if dry_run {
        ("Would remove:", "Would clean: ")
    } else {
        ("Successfully removed", "Cleaned ")
    };
    let assertion = sweep(args)
        .env("CARGO_TARGET_DIR", target.path())
        .assert()
        .success()
        .stdout(contains(remove_msg).and(contains(clean_msg)));

    let output = assertion.get_output();
    assert!(output.stderr.is_empty());
    let amount = std::str::from_utf8(&output.stdout)?
        .lines()
        .last()
        .unwrap()
        .split(clean_msg)
        .nth(1)
        .unwrap();
    let cleaned = amount
        .parse::<human_size::Size>()
        .context(format!("failed to parse amount {amount}"))?
        .to_bytes();

    Ok(cleaned)
}

fn count_cleaned(target: &TempDir, args: &[&str], old_size: u64) -> Result<u64> {
    let cleaned = clean_and_parse(target, args)?;

    // Make sure this is accurate.
    let new_size = get_size(target.path())?;
    // Cargo-sweep and `get_size` appear to have different rounding behavior.
    // Make sure this is within one byte.
    assert!(
        old_size - cleaned - new_size <= 1,
        "new_size={}, old_size={}, cleaned={}",
        new_size,
        old_size,
        cleaned
    );

    Ok(cleaned)
}

fn count_cleaned_dry_run(target: &TempDir, args: &[&str], old_size: u64) -> Result<u64> {
    let mut args = args.to_vec();
    args.push("--dry-run");
    let cleaned = clean_and_parse(target, &args)?;

    let new_size = get_size(target.path())?;
    assert_eq!(old_size, new_size);

    Ok(cleaned)
}

#[test]
fn all_flags() -> Result<(), AnyhowWithContext> {
    let all_combos = [
        ["--time", "0"].as_slice(),
        &["--maxsize", "0"],
        // TODO(#67): enable this test
        // &["--installed"],
    ];

    for args in all_combos {
        let (size, target) = build()?;

        let expected_cleaned = count_cleaned_dry_run(&target, args, size)?;
        assert!(expected_cleaned > 0);

        let actual_cleaned = count_cleaned(&target, args, size)?;
        assert_eq!(actual_cleaned, expected_cleaned);
    }

    Ok(())
}

#[test]
fn stamp_file() -> Result<(), AnyhowWithContext> {
    let (size, target) = build()?;

    // Create a stamp file for --file.
    let assert = sweep(dbg!(&["--stamp", "-v"])).assert().success();
    println!("{}", std::str::from_utf8(&assert.get_output().stdout).unwrap());
    assert!(project_dir().join("sweep.timestamp").exists());

    let args = &["--file"];
    let expected_cleaned = count_cleaned_dry_run(&target, args, size)?;
    assert!(expected_cleaned > 0);

    // For some reason, we delete the stamp file after `--file` :(
    // Recreate it.
    sweep(dbg!(&["--stamp"])).assert().success();

    let actual_cleaned = count_cleaned(&target, args, size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    Ok(())
}
