use bar::ar::{Bar, BarResult};
use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use std::{fs, path::Path};

/// Validator for path inputs
fn file_exists(s: String) -> Result<(), String> {
    match Path::new(&s).exists() {
        true => Ok(()),
        false => Err(format!("The file or directory at {} does not exist", s)),
    }
}

/// Create the `pack` subcommand
fn pack_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("pack")
        .about("Pack a directory into an archive")
        .alias("p")
        .arg(Arg::with_name("input-dir")
            .required(true)
            .multiple(false)
            .takes_value(true)
            .long("input-dir")
            .short("i")
            .help("Choose a full or relative path to the directory that will be compressed into an archive")
            .validator(file_exists)
        )   
        .arg(Arg::with_name("output-file")
            .required(true)
            .takes_value(true)
            .multiple(false)
            .long("output-file")
            .short("o")
            .help("Path to the finished output archive file (careful, if a file already exists, it will be deleted)")
        )
}

fn unpack_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("unpack")
        .alias("u")
        .about("Unpack a .bar archive into a directory")
        .arg(Arg::with_name("input-file")
            .help("Select a full or relative path to an input bar archive")
            .required(true)
            .takes_value(true)
            .multiple(false)
            .validator(file_exists)
            .long("input-file")
            .short("i")
        )
        .arg(Arg::with_name("output-dir")
            .help("Select a full or relative path to the output directory where a directory containing the unpacked contents will go")
            .takes_value(true)
            .multiple(false)
            .required(true)
            .long("output-dir")
            .short("o")
        )
}

fn main() {
    let app = App::new("bar")
        .about("Barchiver\nUtitility to pack, unpack, and manipulate .bar archives")
        .author("Bendi11")
        .version(crate_version!())
        .setting(AppSettings::WaitOnError)
        .subcommand(pack_subcommand())
        .subcommand(unpack_subcommand());

    let matches = app.get_matches();
    match matches.subcommand() {
        ("pack", Some(args)) => pack(args).unwrap(),
        ("unpack", Some(args)) => unpack(args).unwrap(),
        _ => (),
    }
}

/// Pack a directory into a file
fn pack(args: &ArgMatches) -> BarResult<()> {
    let input_dir = args.value_of("input-dir").unwrap();
    let output_file = args.value_of("output-file").unwrap();

    //Open the output file
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output_file)?;
    let back = tempfile::tempfile().unwrap();

    let mut barchiver = Bar::pack(input_dir, back)?; //Pack the directory into a main file
    barchiver.write(&mut output)?;

    Ok(())
}

/// Unpack an archive to a directory
fn unpack(args: &ArgMatches) -> BarResult<()> {
    let input_file = args.value_of("input-file").unwrap();
    let output_dir = args.value_of("output-dir").unwrap();
    let mut barchiver = Bar::unpack(input_file)?; //Pack the directory into a main file
    barchiver.save_unpacked(output_dir)?;

    Ok(())
}
