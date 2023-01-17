use clap::{ArgGroup, Parser};
use std::path::PathBuf;

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
    pub path: Option<PathBuf>,

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

    /// Remove oldest artifacts until the target directory is below the specified size in MB
    #[arg(short, long, value_name = "MAXSIZE_MB")]
    maxsize: Option<u64>,

    /// Apply on all projects below the given path
    #[arg(short, long)]
    pub recursive: bool,

    /// Store timestamp file at the given path, is used by file option
    #[arg(short, long)]
    stamp: bool,

    /// Number of days backwards to keep
    #[arg(short, long)]
    time: Option<u64>,

    /// Toolchains (currently installed by rustup) that should have their artifacts kept
    #[arg(long, value_delimiter = ',')]
    toolchains: Vec<String>,

    /// Turn verbose information on
    #[arg(short, long)]
    pub verbose: bool,
}

impl Args {
    pub fn criterion(&self) -> Criterion {
        match &self {
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
            } => Criterion::MaxSize(*size),
            _ => unreachable!("guaranteed by clap ArgGroup"),
        }
    }
}

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
        assert!(parse("cargo sweep --toolchains 1 2 3").is_err());
    }
}
