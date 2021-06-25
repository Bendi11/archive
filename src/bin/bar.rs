use bar::ar::{Bar, BarResult};
use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use console::style;
use std::{fs, path::Path};

/// An argument with the name "input-file" that validates that its argument exists and only takes one
/// value
fn input_archive_arg() -> Arg<'static, 'static> {
    Arg::with_name("input-file")
        .help("Select a full or relative path to an input bar archive")
        .required(true)
        .takes_value(true)
        .multiple(false)
        .validator(file_exists)
        .long("input-file")
        .short("i")
}

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
        .visible_alias("p")
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
        .arg(Arg::with_name("compression")
            .takes_value(true)
            .multiple(false)
            .long("compression")
            .short("c")
            .help("Select a compression method and quality")
            .possible_values(&[
                "high-gzip",
                "high-deflate",
                "medium-gzip",
                "medium-deflate",
                "fast-gzip",
                "fast-deflate"
            ])
            .default_value("none")
        )

}

fn unpack_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("unpack")
        .visible_alias("u")
        .about("Unpack a .bar archive into a directory")
        .arg(input_archive_arg())
        .arg(Arg::with_name("output-dir")
            .help("Select a full or relative path to the output directory where a directory containing the unpacked contents will go")
            .takes_value(true)
            .multiple(false)
            .required(true)
            .long("output-dir")
            .short("o")
        )
        .arg(input_archive_arg())
}

fn meta_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("meta")
        .about("View metadata of one file or directory")
        .visible_alias("m")
        .visible_alias("view")
        .arg(Arg::with_name("entry-paths")
            .help("A list of paths to fetch the metadata of")
            .multiple(true)
            .takes_value(true)
            .index(1)
            .long("entry-paths")
            .required(true)
            .short("e")
        )
        .arg(input_archive_arg())
}

fn tree_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("tree")
        .visible_alias("t")
        .about("Show the directory tree of the archive, as well as metadata about files")
        .arg(Arg::with_name("show-meta")
            .short("m")
            .long("meta")
            .help("Show metadata along with file and directory entries")
            .takes_value(false)
            .multiple(false)
        )
        .arg(input_archive_arg())       
}

fn main() {
    let app = App::new("bar")
        .about("Barchiver\nUtitility to pack, unpack, and manipulate .bar archives")
        .author("Bendi11")
        .version(crate_version!())
        .setting(AppSettings::WaitOnError)
        .subcommand(pack_subcommand())
        .subcommand(unpack_subcommand())
        .subcommand(meta_subcommand())
        ;

    let matches = app.get_matches();
    match matches.subcommand() {
        ("pack", Some(args)) => pack(args).unwrap(),
        ("unpack", Some(args)) => unpack(args).unwrap(),
        ("meta", Some(args)) => meta(args).unwrap(),
        _ => (),
    }
}

/// Pack a directory into a file
fn pack(args: &ArgMatches) -> BarResult<()> {
    let input_dir = args.value_of("input-dir").unwrap();
    let output_file = args.value_of("output-file").unwrap();
    let compression = args.value_of("compression").unwrap().parse().unwrap();

    //Open the output file
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output_file)?;
    let back = tempfile::tempfile().unwrap();

    let mut barchiver = Bar::pack(input_dir, back, compression)?; //Pack the directory into a main file
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

/// Show metadata about a list of files in an archive
fn meta(args: &ArgMatches) -> BarResult<()> {
    let bar = Bar::unpack(args.value_of("input-file").unwrap())?;
    for arg in args.values_of("entry-paths").unwrap() {
        let meta = match (bar.file(arg), bar.dir(arg)) {
            (Some(file), _) => {
                println!("{}", style(format!("File {}", arg)).bold());
                &file.meta
            },
            (_, Some(dir)) => {
                println!("{}", style(format!("Directory {}", arg)).bold());
                &dir.meta
            },
            (_, _) => {
                eprintln!("{}", style(format!("File or directory {} does not exist in the archive", arg)).red().bold());
                continue
            }
        };
        if let Some(ref last_update) = meta.last_update {
            println!("Last updated on {}", last_update.format("%v at %r"))
        }
        if let Some(ref note) = meta.note {
            println!("{}{}", style("Note: ").bold(), note);
        }
        println!("{}", match meta.used {
            true => "This file has been used",
            false => "This file has not been used"
        });
        println!("{}", style("\n==========\n").white().bold());

    }

    Ok(())
}

