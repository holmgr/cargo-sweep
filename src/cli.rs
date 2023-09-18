use anyhow::anyhow;
use clap::{ArgGroup, Parser};
use std::path::PathBuf;

const MEGABYTE: u64 = 1024 * 1024;

pub fn parse() -> Args {
    SweepArgs::parse().into_args()
}

#[derive(Parser)]
#[command(version, about)]
pub struct SweepArgs {
    #[command(subcommand)]
    sweep: SweepCommand,
}

impl SweepArgs {
    fn into_args(self) -> Args {
        let SweepCommand::Sweep(args) = self.sweep;
        args
    }
}

#[derive(clap::Subcommand)]
pub enum SweepCommand {
    Sweep(Args),
}

#[derive(Parser, Debug)]
#[cfg_attr(test, derive(Default, PartialEq))]
#[command(
    about,
    version,
    group(
        ArgGroup::new("criterion")
            .required(true)
            .args(["stamp", "file", "time", "installed", "toolchains", "maxsize"])
    )
)]
pub struct Args {
    /// Path to check
    pub path: Vec<PathBuf>,

    /// Dry run which will not delete any files
    #[arg(short, long)]
    pub dry_run: bool,

    /// Load timestamp file in the given path, cleaning everything older
    #[arg(short, long)]
    file: bool,

    #[arg(
        long,
        help = "The `recursive` flag defaults to ignoring directories \
            that start with a `.`, `.git` for example is unlikely to include a \
            Cargo project, this flag changes it to look in them"
    )]
    pub hidden: bool,

    /// Keep only artifacts made by Toolchains currently installed by rustup
    #[arg(short, long)]
    installed: bool,

    /// Remove oldest artifacts from the target folder until it's smaller than MAXSIZE
    ///
    /// Unit defaults to MB, examples: --maxsize 500, --maxsize 10GB
    #[arg(short, long, value_name = "MAXSIZE")]
    maxsize: Option<String>,

    /// Apply on all projects below the given path
    #[arg(short, long)]
    pub recursive: bool,

    /// Store timestamp file at the given path, is used by file option
    #[arg(short, long)]
    stamp: bool,

    /// Number of days backwards to keep
    #[arg(short, long, value_name = "DAYS")]
    time: Option<u64>,

    /// Toolchains currently installed by rustup that should have their artifacts kept
    #[arg(long, value_delimiter = ',')]
    toolchains: Vec<String>,

    /// Enable DEBUG logs (use twice for TRACE logs)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Args {
    // Might fail in case parsing size units fails.
    pub fn criterion(&self) -> anyhow::Result<Criterion> {
        Ok(match &self {
            _ if self.stamp => Criterion::Stamp,
            _ if self.file => Criterion::File,
            _ if self.installed => Criterion::Installed,
            _ if !self.toolchains.is_empty() => Criterion::Toolchains(self.toolchains.clone()),
            Self {
                time: Some(time), ..
            } => Criterion::Time(*time),
            Self {
                maxsize: Some(size),
                ..
            } => {
                // Try parsing as `human_size::Size` to accept "MB" and "GB" as units
                // If it fails, fall back to `u64` and use "MB" as the default unit

                let size = size
                    .parse::<human_size::Size>()
                    .map(human_size::Size::to_bytes)
                    .or_else(|_| size.parse::<u64>().map(|size| size * MEGABYTE))
                    .map_err(|_| anyhow!(format!("Failed to parse size '{size}'")))?;

                Criterion::MaxSize(size)
            }
            _ => unreachable!("guaranteed by clap ArgGroup"),
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum Criterion {
    Stamp,
    File,
    Time(u64),
    Installed,
    Toolchains(Vec<String>),
    MaxSize(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper for splitting arguments and providing them to clap
    fn parse(command: &str) -> Result<Args, clap::Error> {
        let command_args = command.split_whitespace();
        dbg!(SweepArgs::try_parse_from(command_args).map(SweepArgs::into_args))
    }

    #[test]
    fn test_argparsing() {
        // Argument is required
        assert!(parse("cargo sweep").is_err());
        assert!(parse("cargo sweep --installed").is_ok());
        assert!(parse("cargo sweep --file").is_ok());
        assert!(parse("cargo sweep --stamp").is_ok());
        assert!(parse("cargo sweep --time 30").is_ok());
        assert!(parse("cargo sweep --toolchains SAMPLE_TEXT").is_ok());
        assert!(parse("cargo sweep --maxsize 100").is_ok());

        assert!(parse("cargo-sweep sweep").is_err());
        assert!(parse("cargo-sweep sweep --installed").is_ok());
        assert!(parse("cargo-sweep sweep --file").is_ok());
        assert!(parse("cargo-sweep sweep --stamp").is_ok());
        assert!(parse("cargo-sweep sweep --time 30").is_ok());
        assert!(parse("cargo-sweep sweep --toolchains SAMPLE_TEXT").is_ok());
        assert!(parse("cargo-sweep sweep --maxsize 100").is_ok());

        // Argument conflicts
        assert!(parse("cargo sweep --installed --maxsize 100").is_err());
        assert!(parse("cargo sweep --file --installed").is_err());
        assert!(parse("cargo sweep --stamp --file").is_err());
        assert!(parse("cargo sweep --time 30 --stamp").is_err());
        assert!(parse("cargo sweep --toolchains SAMPLE_TEXT --time 30").is_err());
        assert!(parse("cargo sweep --maxsize 100 --toolchains SAMPLE_TEXT").is_err());

        // Test if comma separated list is parsed correctly
        let args = Args {
            toolchains: ["1", "2", "3"].map(ToString::to_string).to_vec(),
            ..Args::default()
        };
        assert_eq!(args, parse("cargo sweep --toolchains 1,2,3").unwrap());
    }

    #[test]
    fn test_maxsize_argument_parsing() {
        let test_data = [
            ("3", 3 * 1024 * 1024),
            ("3MB", 3 * 1000 * 1000),
            ("100", 100 * 1024 * 1024),
            ("100MB", 100 * 1000 * 1000),
            ("100MiB", 100 * 1024 * 1024),
            ("100GB", 100 * 1000 * 1000 * 1000),
            ("100GiB", 100 * 1024 * 1024 * 1024),
            ("700TB", 700 * 1000 * 1000 * 1000 * 1000),
            ("700TiB", 700 * 1024 * 1024 * 1024 * 1024),
        ];

        for (input, expected_size) in test_data {
            let input = format!("cargo-sweep sweep --maxsize {input}");
            let result = parse(&input).unwrap().criterion().unwrap();

            if result != Criterion::MaxSize(expected_size) {
                panic!(
                    "Test failed.\n\
                     Input: {input}\n\
                     Expected Size: {expected_size}\n\
                     Got this instead: {result:?}"
                );
            }
        }
    }
}
