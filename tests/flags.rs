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

fn count_cleaned(target: &TempDir, args: &[&str]) -> anyhow::Result<u64> {
    let assertion = sweep(args)
        .env("CARGO_TARGET_DIR", target.path())
        .assert()
        .success()
        .stdout(contains("Successfully removed").and(contains("Cleaned")));
    let output = assertion.get_output();
    assert!(output.stderr.is_empty());
    let amount = std::str::from_utf8(&output.stdout)?
        .lines()
        .last()
        .unwrap()
        .split("Cleaned ")
        .nth(1)
        .unwrap();
    Ok(amount
        .parse::<human_size::Size>()
        .context(format!("failed to parse amount {amount}"))?
        .to_bytes())
}

#[test]
fn remove_all() -> Result<(), AnyhowWithContext> {
    let (size, target) = build()?;
    let cleaned = count_cleaned(&target, &["--time", "0"])?;
    let new_size = get_size(target.path())?;
    // Cargo-sweep and `get_size` appear to have different rounding behavior.
    // Make sure this is within one byte.
    assert!(
        size - cleaned - new_size <= 1,
        "new_size={}, old_size={}, cleaned={}",
        new_size,
        size,
        cleaned
    );

    Ok(())
}

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
