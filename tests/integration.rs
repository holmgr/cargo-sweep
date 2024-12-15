use std::{
    borrow::BorrowMut,
    env::temp_dir,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{Context, Result};
use assert_cmd::{assert::Assert, cargo::cargo_bin, Command};
use fs_extra::dir::{get_size, CopyOptions};
use predicates::{prelude::PredicateBooleanExt, str::contains};
#[allow(unused_imports)]
use pretty_assertions::{assert_eq, assert_ne};
use regex::Regex;
use tempfile::{tempdir, TempDir};
use unindent::unindent;

static CONFLICTING_TESTS_MUTEX: Mutex<()> = Mutex::new(());

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

fn cargo(cmd_current_dir: impl AsRef<Path>) -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.current_dir(cmd_current_dir);
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
fn run(mut cmd: impl BorrowMut<Command>) -> Assert {
    let assert = cmd.borrow_mut().assert().success();
    let out = assert.get_output();
    let str = |s| std::str::from_utf8(s).unwrap();
    print!("{}", str(&out.stdout));
    eprint!("{}", str(&out.stderr));
    assert
}

/// Runs a cargo build of the named project (which is expected to exist as a direct
/// child of the `tests` directory).
/// Returns the size of the build directory, as well as the [TempDir] of the unique
/// temporary build target directory.
fn build(project: &str) -> Result<(u64, TempDir)> {
    let target = tempdir()?;
    let old_size = get_size(target.path())?;
    run(cargo(project_dir(project))
        .arg("build")
        .env("CARGO_TARGET_DIR", target.path()));
    let new_size = get_size(target.path())?;
    assert!(new_size > old_size, "cargo didn't build anything");
    Ok((new_size, target))
}

/// Run the sweep command, cleaning up target files, and then parse the results and
/// return the number of cleaned files. This function takes a `cmd_modifier` which can
/// be used to modify the `sweep` [Command], e.g. adding or changing environment variables.
fn clean_and_parse(
    args: &[&str],
    // Use this to modify the Command generated by the sweep function.
    cmd_modifier: impl Fn(&mut Command) -> &mut Command,
) -> Result<u64> {
    let dry_run = args.iter().any(|&f| f == "-d" || f == "--dry-run");

    let (remove_msg, clean_msg) = if dry_run {
        ("Would remove:", "Would clean: ")
    } else {
        ("Successfully removed", "Cleaned ")
    };
    let assertion =
        run(cmd_modifier(&mut sweep(args))).stdout(contains(remove_msg).and(contains(clean_msg)));

    let output = assertion.get_output();
    assert!(output.stderr.is_empty());

    // Collect all the lines that contain the "clean message".
    let amount_lines: Vec<String> = std::str::from_utf8(&output.stdout)?
        .lines()
        .filter(|line| line.contains(clean_msg))
        .map(|line| {
            line.split(clean_msg)
                .nth(1)
                .unwrap()
                .split_inclusive(' ')
                .take(2)
                .collect::<String>()
        })
        .collect();

    // Turn all those lines into a collected sum of bytes called `clean`.
    let cleaned: u64 = amount_lines
        .iter()
        .map(|amount| {
            amount
                .parse::<human_size::Size>()
                .context(format!("failed to parse amount {amount}"))
                .unwrap()
                .to_bytes()
        })
        .sum();

    Ok(cleaned)
}

fn count_cleaned(target: &TempDir, args: &[&str], old_size: u64) -> Result<u64> {
    let cleaned = clean_and_parse(args, |cmd| cmd.env("CARGO_TARGET_DIR", target.path()))?;
    assert_sweeped_size(target.path(), cleaned, old_size)?;
    Ok(cleaned)
}

/// Assert that the size of the target directory has the expected size
/// (within a small margin of error).
fn assert_sweeped_size(
    path: impl AsRef<Path>,
    // The number of bytes that were sweeped ("cleaned").
    cleaned: u64,
    // The size of the target directory (in bytes) before the sweep.
    old_size: u64,
) -> Result<()> {
    // Make sure that the expected target directory size is accurate.
    let new_size = get_size(path)?;
    // Due to rounding and truncation we might have an inexact result. Make sure
    // these are within 1% of each other, but don't require an exact match.
    let calculated_size = old_size - cleaned;
    let diff = new_size.abs_diff(calculated_size);
    let one_percent = old_size as f64 * 0.01;
    assert!(
        diff <= one_percent as u64,
        "new_size={new_size}, old_size={old_size}, cleaned={cleaned}, diff={diff}, 1%={one_percent}",
    );
    Ok(())
}

fn count_cleaned_dry_run(target: &TempDir, args: &[&str], old_size: u64) -> Result<u64> {
    let mut args = args.to_vec();
    args.push("--dry-run");
    let cleaned = clean_and_parse(&args, |cmd| cmd.env("CARGO_TARGET_DIR", target.path()))?;

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
    let _lock = CONFLICTING_TESTS_MUTEX.lock();

    let (size, target) = build("sample-project")?;
    let stamp_file_exists = || {
        project_dir("sample-project")
            .join("sweep.timestamp")
            .exists()
    };

    // Create a stamp file for --file.
    let assert = run(sweep(&["--stamp"]).env("CARGO_TARGET_DIR", target.path()));
    println!(
        "{}",
        std::str::from_utf8(&assert.get_output().stdout).unwrap()
    );

    assert!(stamp_file_exists(), "failed to create stamp file");

    let args = &["--file"];
    let expected_cleaned = count_cleaned_dry_run(&target, args, size)?;
    assert!(expected_cleaned > 0);

    assert!(stamp_file_exists(), "failed to keep stamp file on dry run");

    let actual_cleaned = count_cleaned(&target, args, size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    assert!(!stamp_file_exists(), "failed to yeet stamp file after run");

    Ok(())
}

#[test]
fn empty_project_output() -> TestResult {
    let (_size, target) = build("sample-project")?;

    let assert = run(sweep(&["--maxsize", "0"])
        .current_dir(test_dir().join("sample-project"))
        .env("CARGO_TARGET_DIR", target.path()));

    let output = std::str::from_utf8(&assert.get_output().stdout).unwrap();

    // Please note: The output from the sweep command is platform dependent. The regular
    // expression tries to take that into account by letting the file output order vary.
    let pattern = unindent(
        r#"\[DEBUG\] cleaning: ".+" with remove_older_until_fits
        \[DEBUG\] size_to_remove: .+
        \[DEBUG\] Sizing: ".+debug" with total_disk_space_in_a_profile
        \[DEBUG\] Hashs by time: \[
            \(
                .+,
                ".+",
            \),
        \]
        \[DEBUG\] cleaning: ".+debug" with remove_not_built_with_in_a_profile
        \[DEBUG\] Successfully removed: ".+sample_project.+"
        (\s*\S*)*
        \[INFO\] Cleaned .+ from ".+""#,
    );

    assert!(
        regex_matches(&pattern, output),
        "failed to output with regex pattern\npattern = {pattern}"
    );

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
    let assert = run(sweep(&["--installed"])
        .env("PATH", test_dir())
        .env("CARGO_TARGET_DIR", tempdir.path()));
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

/// This scenario used to panic: https://github.com/holmgr/cargo-sweep/issues/117
#[test]
fn stamp_file_not_found() -> TestResult {
    let _lock = CONFLICTING_TESTS_MUTEX.lock();

    sweep(&["--file"])
        .current_dir(test_dir().join("sample-project"))
        .assert()
        .failure()
        .stderr(contains("failed to read stamp file").and(contains("panicked").not()));
    Ok(())
}

#[test]
fn path() -> TestResult {
    let (_, target) = build("sample-project")?;
    let mut cmd = Command::new(cargo_bin("cargo-sweep"));

    cmd.arg("sweep").arg("--installed").current_dir(temp_dir());

    // Pass `path` as an argument, instead of `current_dir` like it normally is.
    let assert = run(cmd
        .arg(project_dir("sample-project"))
        .env("CARGO_TARGET_DIR", target.path()));
    assert.stdout(contains("Cleaned"));

    Ok(())
}

#[test]
fn usage() -> TestResult {
    trycmd::TestCases::new().case("tests/*.trycmd");
    Ok(())
}

/// This test is a bit more verbose than the other tests because it uses a slightly
/// different mechanism for setting up and sweeping / cleaning.
///
/// Step-by-step:
///
/// * Create a new temporary directory. Other tests only output the built files into
///   the target directory, but this test copies the whole template project into the
///   target directory.
/// * Build the two different (nested) projects using cargo build.
/// * Do a dry run sweep, asserting that nothing was actually cleaned.
/// * Do a proper sweep, asserting that the target directory size is back down to its
///   expected size.
///
/// Please note that there is some ceremony involved, namely correctly setting
/// `CARGO_TARGET_DIR` to `unset`, so as not to clash with users who use `CARGO_TARGET_DIR`
///   when they invoke `cargo test` for running these tests.
#[test]
fn recursive_multiple_root_workspaces() -> TestResult {
    let temp_workspace_dir = tempdir()?;

    let nested_workspace_dir = test_dir().join("nested-root-workspace");
    let options = CopyOptions::default();

    // Copy the whole nested-root-workspace folder (and its content) into the new temp dir,
    // and then `cargo build` and run the sweep tests inside that directory.
    fs_extra::copy_items(
        &[&nested_workspace_dir],
        temp_workspace_dir.path(),
        &options,
    )?;

    let old_size = get_size(temp_workspace_dir.path())?;

    // Build bin-crate
    run(cargo(
        temp_workspace_dir
            .path()
            .join("nested-root-workspace/bin-crate"),
    )
    // If someone has built & run these tests with CARGO_TARGET_DIR,
    // we need to override that.
    .env_remove("CARGO_TARGET_DIR")
    .arg("build"));

    let intermediate_build_size = get_size(temp_workspace_dir.path())?;
    assert!(intermediate_build_size > old_size);

    // Build workspace crates
    run(
        cargo(temp_workspace_dir.path().join("nested-root-workspace"))
            // If someone has built & run these tests with CARGO_TARGET_DIR,
            // we need to override that.
            .env_remove("CARGO_TARGET_DIR")
            .arg("build"),
    );

    let final_build_size = get_size(temp_workspace_dir.path())?;
    assert!(final_build_size > intermediate_build_size);

    // Measure the size of the nested root workspace and the bin crate before cargo-sweep is invoked.
    let pre_clean_size_nested_root_workspace =
        get_size(temp_workspace_dir.path().join("nested-root-workspace"))?;
    let pre_clean_size_bin_create = get_size(
        temp_workspace_dir
            .path()
            .join("nested-root-workspace/bin-crate"),
    )?;

    // Run a dry-run of cargo-sweep ("clean") in the target directory (recursive)
    let args = &["-r", "--time", "0", "--dry-run"];
    let expected_cleaned = clean_and_parse(args, |cmd| {
        // If someone has built & run these tests with CARGO_TARGET_DIR,
        // we need to override that.
        cmd.env_remove("CARGO_TARGET_DIR")
            .current_dir(temp_workspace_dir.path())
    })?;
    assert!(expected_cleaned > 0);
    let size_after_dry_run_clean = get_size(temp_workspace_dir.path())?;
    // Make sure that nothing was actually cleaned
    assert_eq!(final_build_size, size_after_dry_run_clean);

    // Run a proper cargo-sweep ("clean") in the target directory (recursive)
    let args = &["-r", "--time", "0"];
    let actual_cleaned = clean_and_parse(args, |cmd| {
        // If someone has built & run these tests with CARGO_TARGET_DIR,
        // we need to override that.
        cmd.env_remove("CARGO_TARGET_DIR")
            .current_dir(temp_workspace_dir.path())
    })?;
    assert_sweeped_size(temp_workspace_dir.path(), actual_cleaned, final_build_size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    // Measure the size of the nested root workspace and the bin crate after cargo-sweep is invoked,
    // and assert that both of their sizes have been reduced.
    // This works because by default cargo generates the `target/` directory in a sub-directory
    // of the package root.
    let post_clean_size_nested_root_workspace =
        get_size(temp_workspace_dir.path().join("nested-root-workspace"))?;
    let post_clean_size_bin_crate = get_size(
        temp_workspace_dir
            .path()
            .join("nested-root-workspace/bin-crate"),
    )?;
    assert!(post_clean_size_nested_root_workspace < pre_clean_size_nested_root_workspace, "The size of the nested root workspace create has not been reduced after running cargo-sweep.");
    assert!(
        post_clean_size_bin_crate < pre_clean_size_bin_create,
        "The size of the bin create has not been reduced after running cargo-sweep."
    );

    Ok(())
}

/// This test follows the logic of the recursive multiple root test, however, instead of recursing it passes each workspace individually.
#[test]
fn multiple_paths() -> TestResult {
    let project_root_path = tempdir()?;

    let crate_dir = test_dir().join("sample-project");
    let options = CopyOptions::default();

    let project_names = ["sample-project-1", "sample-project-2"];

    // Copy the sample project folder twice
    // and then `cargo build` and run the sweep tests inside that directory.
    for project_name in &project_names {
        fs_extra::dir::copy(&crate_dir, project_root_path.path(), &options)?;
        fs::rename(
            project_root_path
                .path()
                .join(crate_dir.file_name().unwrap()),
            dbg!(project_root_path.path().join(project_name)),
        )?;
    }

    let old_size = get_size(project_root_path.path())?;

    // Build crates
    for path in &project_names {
        run(cargo(project_root_path.path().join(path))
            // If someone has built & run these tests with CARGO_TARGET_DIR,
            // we need to override that.
            .env_remove("CARGO_TARGET_DIR")
            .arg("build"));
    }

    let final_build_size = get_size(project_root_path.path())?;
    // Calculate the size of each individual crate
    let final_built_crates_size =
        project_names.map(|path| get_size(project_root_path.path().join(path)).unwrap());

    assert!(final_build_size > old_size);

    // Measure the size of the crates before cargo-sweep is invoked.
    // Run a dry-run of cargo-sweep ("clean") in the target directory of all the crates
    let mut args = vec!["--time", "0", "--dry-run"];
    args.append(&mut project_names.to_vec());

    let expected_cleaned = clean_and_parse(&args, |cmd| {
        // If someone has built & run these tests with CARGO_TARGET_DIR,
        // we need to override that.
        cmd.env_remove("CARGO_TARGET_DIR")
            .current_dir(project_root_path.path())
    })?;

    assert!(expected_cleaned > 0);
    let size_after_dry_run_clean = get_size(project_root_path.path())?;
    // Make sure that nothing was actually cleaned
    assert_eq!(final_build_size, size_after_dry_run_clean);

    // Run a proper cargo-sweep ("clean") in the target directories
    let mut args = vec!["--time", "0"];
    args.append(&mut project_names.to_vec());

    let actual_cleaned = clean_and_parse(&args, |cmd| {
        // If someone has built & run these tests with CARGO_TARGET_DIR,
        // we need to override that.
        cmd.env_remove("CARGO_TARGET_DIR")
            .current_dir(project_root_path.path())
    })?;

    assert_sweeped_size(project_root_path.path(), actual_cleaned, final_build_size)?;
    assert_eq!(actual_cleaned, expected_cleaned);

    // Assert that each crate was cleaned
    let cleaned_crates_size =
        project_names.map(|path| get_size(project_root_path.path().join(path)).unwrap());

    final_built_crates_size
        .iter()
        .zip(cleaned_crates_size.iter())
        .for_each(|(a, b)| assert!(a > b));

    Ok(())
}

#[test]
fn multiple_paths_and_stamp_errors() -> TestResult {
    let project_root_path = tempdir()?;

    let crate_dir = test_dir().join("sample-project");
    let options = CopyOptions::default();

    let project_names = ["sample-project-1", "sample-project-2"];

    // Copy the sample project folder twice
    // and then `cargo build` and run the sweep tests inside that directory.
    for project_name in &project_names {
        fs_extra::dir::copy(&crate_dir, project_root_path.path(), &options)?;
        fs::rename(
            project_root_path
                .path()
                .join(crate_dir.file_name().unwrap()),
            dbg!(project_root_path.path().join(project_name)),
        )?;
    }

    let mut args = vec!["--stamp"];
    args.append(&mut project_names.to_vec());

    sweep(&args)
        .env_remove("CARGO_TARGET_DIR")
        .current_dir(project_root_path.path())
        .assert()
        .failure()
        .stderr(contains(
            "Using multiple paths and --stamp is currently unsupported",
        ));

    Ok(())
}

#[test]
fn check_toolchain_listing_on_multiple_projects() -> TestResult {
    let args = &["sweep", "--dry-run", "--recursive", "--installed"];
    let assert = run(Command::new(cargo_bin("cargo-sweep"))
        .args(args)
        .current_dir("tests/"));

    let stdout = std::str::from_utf8(&assert.get_output().stdout).unwrap();
    let lines = stdout
        .lines()
        .filter(|line| line.starts_with("[INFO]"))
        .collect::<Vec<_>>();

    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[0].trim(),
        "[INFO] Searching recursively for Rust project folders"
    );
    assert!(lines[1].starts_with("[INFO] Using all installed toolchains:"));
    assert!(lines[2].starts_with("[INFO] Would clean:"));
    assert!(lines[3].starts_with("[INFO] Would clean:"));

    Ok(())
}
