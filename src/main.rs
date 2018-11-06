extern crate clap;

use clap::{App, Arg, SubCommand};

fn main() {
    // TODO: Add path argument
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
                ),
        ).get_matches();

    if let Some(matches) = matches.subcommand_matches("sweep") {
        // First unwrap is safe due to clap check.
        let days_to_keep: u32 = matches
            .value_of("time")
            .unwrap()
            .parse()
            .expect("Invalid time format");

        println!("Days to keep: {}", days_to_keep);
    }

    // Continued program logic goes here...
}
