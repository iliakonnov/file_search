#![allow(dead_code)]
#![warn(
    clippy::all,
    clippy::correctness,
    clippy::style,
    clippy::complexity,
    clippy::perf,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};

mod database;
mod find;
mod mixed;
mod model;
mod offseted_reader;
mod parser;

fn update(_args: &ArgMatches) {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let settings = parser::Settings {
        bypass_errors: true,
    };
    let parser = parser::Parser::new(settings);
    match parser.parse(&mut reader) {
        Ok(res) => {
            let _out = std::fs::File::create("./ouput.json").unwrap();
            println!("{}", res.len());
        }
        Err(err) => {
            eprintln!("{}", err);
        }
    };
}

fn fallback(args: &ArgMatches) {
    dbg!(args);
}

#[allow(clippy::match_same_arms)]
fn main() {
    let matches = App::new("File search")
        .version(crate_version!())
        .about("Find any file by name in your btrfs subvolumes")
        .arg(Arg::with_name("database")
            .short("d")
            .long("db")
            .takes_value(true)
            .global(true)
            .help("Path to database"))
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .global_settings(&[
            AppSettings::ColoredHelp,
            AppSettings::VersionlessSubcommands
        ])
        .subcommand(SubCommand::with_name("initialize")
            .about("Initializes empty database"))
        .subcommand(SubCommand::with_name("update")
            .about("Reads stream from btrfs send and updates the database")
            .arg(Arg::with_name("pipe")
                .long("pipe")
                .short("p")
                .help("Read stream from stdin"))
            .arg(Arg::with_name("snapshot")
                .conflicts_with("pipe")
                .long("snapshot")
                .short("s")
                .help("Path to snapshots. Conflicts with `pipe`"))
            .arg(Arg::with_name("subvolume")
                .help("Update only specified subvolumes")))
        .subcommand(SubCommand::with_name("query")
            .about("Find files matching the query")
            .after_help("Query language:")
            .arg(Arg::with_name("query")
                .multiple(true)
                .help("Your query")))
        .subcommand(SubCommand::with_name("macros")
            .about("Manage macroses")
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .subcommand(SubCommand::with_name("add")
                .about("Add new macro")
                .after_help("Precompiled macroses are working faster, but they increase the size of the database")
                .arg(Arg::with_name("precompile")
                    .long("precompile")
                    .short("c")
                    .help("Precompile this macro"))
                .arg(Arg::with_name("name")
                    .help("Name of this macro"))
                .arg(Arg::with_name("query")
                    .multiple(true)
                    .help("Query. See `query` subcommand for help")))
            .subcommand(SubCommand::with_name("remove")
                .about("Remove macro")
                .arg(Arg::with_name("name")))
            .subcommand(SubCommand::with_name("list")
                .about("List all macroses")))
        .subcommand(SubCommand::with_name("subvolume")
            .about("Manage indexed subvolumes")
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .subcommand(SubCommand::with_name("add")
                .about("Add new subvolume")
                .arg(Arg::with_name("path")
                    .help("Path to subvolume")))
            .subcommand(SubCommand::with_name("remove")
                .about("Remove subvolume")
                .arg(Arg::with_name("path")))
            .subcommand(SubCommand::with_name("list")
                .about("List all subvolumes")))
        .get_matches();

    match matches.subcommand() {
        ("initialize", Some(sub)) => fallback(sub),
        ("update", Some(sub)) => update(sub),
        ("query", Some(sub)) => fallback(sub),
        ("macros", Some(sub)) => match sub.subcommand() {
            ("add", Some(sub)) => fallback(sub),
            ("remove", Some(sub)) => fallback(sub),
            ("list", Some(sub)) => fallback(sub),
            _ => unreachable!(),
        },
        ("subvolume", Some(sub)) => match sub.subcommand() {
            ("add", Some(sub)) => fallback(sub),
            ("remove", Some(sub)) => fallback(sub),
            ("list", Some(sub)) => fallback(sub),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}
