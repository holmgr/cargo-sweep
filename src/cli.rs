use clap::{
    app_from_crate, crate_authors, crate_description, crate_name, crate_version, Arg, ArgGroup,
    ArgMatches, SubCommand,
};

pub fn arg_matches() -> ArgMatches<'static> {
    app_from_crate!()
        .subcommand(
            SubCommand::with_name("sweep")
                .arg(
                    Arg::with_name("verbose")
                        .short("v")
                        .long("verbose")
                        .help("Turn verbose information on"),
                )
                .arg(
                    Arg::with_name("recursive")
                        .short("r")
                        .long("recursive")
                        .help("Apply on all projects below the given path"),
                )
                .arg(
                    Arg::with_name("hidden")
                        .long("hidden")
                        .help("The `recursive` flag defaults to ignoring directories \
                        that start with a `.`, `.git` for example is unlikely to include a \
                        Cargo project, this flag changes it to look in them."),
                )
                .arg(
                    Arg::with_name("dry-run")
                        .short("d")
                        .long("dry-run")
                        .help("Dry run which will not delete any files"),
                )
                .arg(
                    Arg::with_name("stamp")
                        .short("s")
                        .long("stamp")
                        .help("Store timestamp file at the given path, is used by file option"),
                )
                .arg(
                    Arg::with_name("file")
                        .short("f")
                        .long("file")
                        .help("Load timestamp file in the given path, cleaning everything older"),
                )
                .arg(
                    Arg::with_name("installed")
                        .short("i")
                        .long("installed")
                        .help("Keep only artifacts made by Toolchains currently installed by rustup")
                )
                .arg(
                    Arg::with_name("toolchains")
                        .long("toolchains")
                        .value_name("toolchains")
                        .help("Toolchains (currently installed by rustup) that should have their artifacts kept.")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("maxsize")
                        .long("maxsize")
                        .value_name("maxsize")
                        .help("Remove oldest artifacts until the target directory is below the specified size in MB")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("time")
                        .short("t")
                        .long("time")
                        .value_name("days")
                        .help("Number of days backwards to keep")
                        .takes_value(true),
                )
                .group(
                    ArgGroup::with_name("timestamp")
                        .args(&["stamp", "file", "time", "installed", "toolchains", "maxsize"])
                        .required(true),
                )
                .arg(
                    Arg::with_name("path")
                        .index(1)
                        .value_name("path")
                        .help("Path to check"),
                ),
        )
        .get_matches()
}
