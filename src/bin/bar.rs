use bar::ar::{Bar, BarResult};
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand, crate_version};
use std::{fs::{self, File}, io::{BufReader, BufWriter}, path::Path};

/// Create the `pack` subcommand
fn pack_subcommand() -> clap::App<'static, 'static> {
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
            .validator(|s| match Path::new(&s).exists() {
                true => Ok(()),
                false => Err(format!("The input directory at {} does not exist", s))
            })
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

fn main() {
    let app = App::new("bar")
        .about("Utitility to pack, unpack, and manipulate .bar archives")
        .author("Bendi11")
        .version(crate_version!())
        .subcommand(pack_subcommand());
    let matches = app.get_matches(); 
    match matches.subcommand() {
        ("pack", Some(args)) => pack(args).unwrap(),
        _ => ()
    }
}

/// Pack a directory into a file
fn pack(args: &ArgMatches) -> BarResult<()> {
    let input_dir = args.value_of("input-dir").unwrap();
    let output_file = args.value_of("output-file").unwrap();

    //Open the output file
    let mut output = fs::OpenOptions::new().write(true).create(true).truncate(true).open(output_file)?;
    let back = tempfile::tempfile().unwrap();
    

    let mut barchiver = Bar::pack(input_dir, back)?; //Pack the directory into a main file
    barchiver.write(&mut output)?;

    Ok(())
}
