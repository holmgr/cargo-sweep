use std::{
    borrow::BorrowMut,
    env::temp_dir,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use assert_cmd::Command;
use assert_cmd::{assert::Assert, cargo::cargo_bin};
use fs_extra::dir::get_size;
use predicates::{
    prelude::PredicateBooleanExt,
    str::{contains, is_empty},
};
#[allow(unused_imports)]
use pretty_assertions::{assert_eq, assert_ne};
use regex::Regex;
use tempfile::{tempdir, TempDir};
use unindent::unindent;

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
        .arg("--verbose")
        .args(args);
    cmd
}

/// Sets the target folder and runs the given command
fn run<'a>(mut cmd: impl BorrowMut<Command>, target: impl Into<Option<&'a Path>>) -> Assert {
    if let Some(target) = target.into() {
        cmd.borrow_mut().env("CARGO_TARGET_DIR", target);
    }
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
    run(cargo(project).arg("build"), target.path());
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
    let assertion =
        run(sweep(args), target.path()).stdout(contains(remove_msg).and(contains(clean_msg)));

    let output = assertion.get_output();
    assert!(output.stderr.is_empty());

    // Extract the size from the last line of stdout, example:
    // - stdout: "[INFO] Would clean: 9.43 KiB from "/home/user/project/target"
    // - extracted amount: "9.43 KiB "
    let amount = std::str::from_utf8(&output.stdout)?
        .lines()
        .last()
        .unwrap()
        .split(clean_msg)
        .nth(1)
        .unwrap()
        .split_inclusive(' ')
        .take(2)
        .collect::<String>();

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
        "new_size={new_size}, old_size={old_size}, cleaned={cleaned}, diff={diff}, 1%={one_percent}",
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

fn regex_matches(pattern: &str, text: &str) -> bool {
    let pattern = Regex::new(pattern).expect("Failed to compile regex pattern");
    pattern.is_match(text)
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
    let assert = run(sweep(&["--stamp"]), target.path());
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
    run(sweep(&["--stamp"]), target.path());

    let actual_cleaned = count_cleaned(&target, args, size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    Ok(())
}

#[test]
fn empty_project_output() -> TestResult {
    let (_size, target) = build("sample-project")?;

    let assert = run(
        sweep(&["--maxsize", "0"]).current_dir(test_dir().join("sample-project")),
        target.path(),
    );

    let output = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    let pattern = unindent(
        r#"\[DEBUG\] cleaning: ".+" with remove_older_until_fits
        \[DEBUG\] size_to_remove: .+
        \[DEBUG\] Sizing: ".+/debug" with total_disk_space_in_a_profile
        \[DEBUG\] Hashs by time: \[
            \(
                .+,
                ".+",
            \),
        \]
        \[DEBUG\] cleaning: ".+/debug" with remove_not_built_with_in_a_profile
        \[DEBUG\] Successfully removed: ".+/debug/deps/libsample_project-.+\.rlib"
        \[DEBUG\] Successfully removed: ".+/debug/deps/libsample_project-.+\.rmeta"
        \[DEBUG\] Successfully removed: ".+/debug/deps/sample_project-.+\.d"
        \[DEBUG\] Successfully removed: ".+/debug/.fingerprint/sample-project-.+"
        \[INFO\] Cleaned .+ from ".+""#,
    );

    assert!(
        regex_matches(&pattern, output),
        "failed to match pattern with output"
    );

    Ok(())
}

#[test]
fn hidden() -> TestResult {
    // This path is so strange because we use CARGO_TARGET_DIR to set the target to a temporary directory.
    // So we can't let cargo-sweep discover any other projects, or it will think they share the same directory as this hidden project.
    let (size, target) = build("fresh-prefix/.hidden/hidden-project")?;
    let run = |args| {
        run(
            sweep(args).current_dir(test_dir().join("fresh-prefix")),
            target.path(),
        )
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
    let assert = run(
        sweep(&["--installed"]).env("PATH", test_dir()),
        tempdir.path(),
    );
    assert.stdout(contains("oh no an error"));

    Ok(())
}

#[test]
fn error_status() -> TestResult {
    sweep(&["--installed"])
        .current_dir(temp_dir())
        .assert()
        .failure()
        .stderr(contains("Cargo.toml` does not exist"));
    Ok(())
}

fn golden_reference(args: &[&str], file: &str) -> TestResult {
    let mut cmd = Command::new(cargo_bin("cargo-sweep"));
    let mut assert = run(cmd.args(args), None);

    assert = assert.stderr(is_empty());
    let actual = std::str::from_utf8(&assert.get_output().stdout)?;

    if std::env::var("BLESS").as_deref() == Ok("1") {
        fs::write(file, actual)?;
    } else {
        let mut expected = fs::read_to_string(file).context("failed to read usage file")?;
        content_normalize(&mut expected);
        assert_eq!(actual, expected);
    }
    Ok(())
}

#[test]
fn path() -> TestResult {
    let (_, target) = build("sample-project")?;
    let mut cmd = Command::new(cargo_bin("cargo-sweep"));

    cmd.arg("sweep").arg("--installed").current_dir(temp_dir());

    // Pass `path` as an argument, instead of `current_dir` like it normally is.
    let assert = run(cmd.arg(project_dir("sample-project")), target.path());
    assert.stdout(contains("Cleaned"));

    Ok(())
}

fn content_normalize(content: &mut String) {
    if !cfg!(windows) {
        *content = content.replace("cargo-sweep.exe", "cargo-sweep");
    }
    *content = content.replace("\r\n", "\n");
}

#[test]
fn subcommand_usage() -> TestResult {
    golden_reference(&["sweep", "-h"], "tests/usage.txt")
}

#[test]
fn standalone_usage() -> TestResult {
    golden_reference(&["-h"], "tests/standalone-usage.txt")
}
