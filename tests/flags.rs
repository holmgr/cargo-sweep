use std::{
    borrow::BorrowMut,
    fmt::Debug,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
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
type TestResult = Result<(), AnyhowWithContext>;

fn test_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn project_dir(project: &str) -> PathBuf {
    test_dir().join(project)
}

fn cargo(project: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.current_dir(project_dir(project));
    cmd
}

fn sweep(args: &[&str]) -> Command {
    let mut cmd = Command::new(cargo_bin("cargo-sweep"));
    cmd.arg("sweep")
        .current_dir(project_dir("sample-project"))
        .args(args);
    cmd
}

fn run(mut cmd: impl BorrowMut<Command>) -> Assert {
    let assert = cmd.borrow_mut().assert().success();
    let out = assert.get_output();
    let str = |s| std::str::from_utf8(s).unwrap();
    print!("{}", str(&out.stdout));
    eprint!("{}", str(&out.stderr));
    assert
}

/// Returns the size of the build directory.
fn build(project: &str) -> Result<(u64, TempDir)> {
    let target = tempdir()?;
    let old_size = get_size(target.path())?;
    run(cargo(project)
        .arg("build")
        .env("CARGO_TARGET_DIR", target.path()));
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
    let assertion = run(sweep(args).env("CARGO_TARGET_DIR", target.path()))
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
    // Due to rounding and truncation we might have an inexact result. Make sure these are within 1% of each other,
    // but don't require an exact match.
    let calculated_size = old_size - cleaned;
    let diff = new_size.abs_diff(calculated_size);
    let one_percent = old_size as f64 * 0.01;
    assert!(
        diff <= one_percent as u64,
        "new_size={}, old_size={}, cleaned={}, diff={diff}, 1%={one_percent}",
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
fn all_flags() -> TestResult {
    let all_combos = [
        ["--time", "0"].as_slice(),
        &["--maxsize", "0"],
        // TODO(#67): enable this test
        // &["--installed"],
    ];

    for args in all_combos {
        let (size, target) = build("sample-project")?;

        let expected_cleaned = count_cleaned_dry_run(&target, args, size)?;
        assert!(expected_cleaned > 0);

        let actual_cleaned = count_cleaned(&target, args, size)?;
        assert_eq!(actual_cleaned, expected_cleaned);
    }

    Ok(())
}

#[test]
fn stamp_file() -> TestResult {
    let (size, target) = build("sample-project")?;

    // Create a stamp file for --file.
    let assert = run(sweep(dbg!(&["--stamp", "-v"])));
    println!(
        "{}",
        std::str::from_utf8(&assert.get_output().stdout).unwrap()
    );
    assert!(project_dir("sample-project")
        .join("sweep.timestamp")
        .exists());

    let args = &["--file"];
    let expected_cleaned = count_cleaned_dry_run(&target, args, size)?;
    assert!(expected_cleaned > 0);

    // For some reason, we delete the stamp file after `--file` :(
    // Recreate it.
    run(sweep(&["--stamp"]));

    let actual_cleaned = count_cleaned(&target, args, size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    Ok(())
}

#[test]
fn hidden() -> TestResult {
    // This path is so strange because we use CARGO_TARGET_DIR to set the target to a temporary directory.
    // So we can't let cargo-sweep discover any other projects, or it will think they share the same directory as this hidden project.
    let (size, target) = build("fresh-prefix/.hidden/hidden-project")?;
    let run = |args| {
        run(sweep(args)
            .current_dir(test_dir().join("fresh-prefix"))
            .env("CARGO_TARGET_DIR", target.path()))
    };

    run(&["--maxsize", "0", "-r"]);
    assert_eq!(get_size(target.path())?, size);

    run(&["--maxsize", "0", "-r", "--hidden"]);
    assert!(
        get_size(target.path())? < size,
        "old_size={}, new_size={}",
        size,
        get_size(target.path())?
    );

    Ok(())
}

#[test]
#[cfg(unix)]
/// Setup a PATH that has a rustc that always gives an error. Make sure we see the error output.
fn error_output() -> TestResult {
    use std::io::ErrorKind;
    use which::which;

    let cargo = which("cargo")?;
    match std::os::unix::fs::symlink(cargo, test_dir().join("cargo")) {
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }

    let (_, tempdir) = build("sample-project")?;
    let assert = run(sweep(&["--installed"]).env("PATH", test_dir()).env("CARGO_TARGET_DIR", tempdir.path()));
    assert.stdout(contains("oh no an error"));

    Ok(())
}
