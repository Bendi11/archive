use bar::ar::{Bar, BarErr, BarResult, entry::{self, Entry}};
use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use console::{Color, Style, style};
use dialoguer::theme::ColorfulTheme;
use std::{fs, path::Path};

/// An argument with the name "input-file" that validates that its argument exists and only takes one
/// value
fn input_archive_arg(idx: u64) -> Arg<'static, 'static> {
    Arg::with_name("input-file")
        .help("Select a full or relative path to an input bar archive")
        .required(true)
        .takes_value(true)
        .multiple(false)
        .validator(file_exists)
        .long("input-file")
        .short("i")
        .index(idx)
}

/// Validator for path inputs
fn file_exists(s: String) -> Result<(), String> {
    match Path::new(&s).exists() {
        true => Ok(()),
        false => Err(format!("The file or directory at {} does not exist", s)),
    }
}

/// Output directory argument
fn output_dir_arg(idx: u64) -> Arg<'static, 'static> {
    Arg::with_name("output-dir")
        .help("Select a full or relative path to the directory that output files will be written to. Requires the directory to exist")
        .next_line_help(true)
        .takes_value(true)
        .multiple(false)
        .required(true)
        .long("output-dir")
        .short("o")
        .validator(file_exists)
        .index(idx)
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
                "fast-deflate",
                "none",
            ])
            .default_value("none")
        )

}

fn unpack_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("unpack")
        .visible_alias("u")
        .about("Unpack a .bar archive into a directory")
        .arg(input_archive_arg(1))
        .arg(output_dir_arg(2))
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
            .index(2)
            .long("entry-paths")
            .required(true)
            .short("e")
        )
        .arg(input_archive_arg(1))
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
        .arg(input_archive_arg(1))       
}

fn extract_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("extract")
        .about("Extract a file from a packed archive")
        .arg(input_archive_arg(1))
        .visible_alias("e")
        .arg(Arg::with_name("decompress")
            .short("d")
            .long("decompress")
            .help("Decompress files [on/true] or extract their compressed data without decompressing [off/false]")
            .default_value("on")
            .possible_values(&["on", "true", "off", "false"])
            .multiple(false)
            .takes_value(true)
        )
        .arg(Arg::with_name("extracted-files")
            .help("A list of files to extract from the archive file")
            .short("e")
            .long("extract")
            .index(3)
            .multiple(true)
            .takes_value(true)
            .required(true)
        )
        .arg(Arg::with_name("update-as-used")
            .help("Select wether to update the extracted file's metadata as used")
            .takes_value(false)
            .multiple(false)
            .long("consume")
            .short("c")
        )
        .arg(output_dir_arg(2))
}

fn edit_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("edit")
        .visible_alias("ed")
        .about("View or edit a specific entry's metadata like notes, use, and name")
        .arg(input_archive_arg(1))
        .arg(Arg::with_name("entry")
            .short("e")
            .long("entry")
            .help("Path to a file or directory in the archive to edit the metadata of")
            .required(true)
            .multiple(false)
            .takes_value(true)
            .index(2)
        )
}

fn main() {
    let app = App::new("bar")
        .about("A utility to pack, unpack, and manipulate .bar archives")
        .author("Bendi11")
        .version(crate_version!())
        .setting(AppSettings::WaitOnError)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(pack_subcommand())
        .subcommand(unpack_subcommand())
        .subcommand(meta_subcommand())
        .subcommand(tree_subcommand())
        .subcommand(extract_subcommand())
        .subcommand(edit_subcommand())
        ;

    let matches = app.get_matches();
    match match matches.subcommand() {
        ("pack", Some(args)) => pack(args),
        ("unpack", Some(args)) => unpack(args),
        ("meta", Some(args)) => meta(args),
        ("tree", Some(args)) => tree(args),
        ("extract", Some(args)) => extract(args),
        ("edit", Some(args)) => edit(args),
        _ => unreachable!()
    } {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}{}", style(format!("An error occurred in subcommand {}: ", matches.subcommand().0)).bold().white(), style(e).red());
        }
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
    barchiver.save(&mut output)?;

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
                println!("{}{}", style("File: ").white(), style(arg).bold().green());
                &file.meta
            },
            (_, Some(dir)) => {
                println!("{}{}", style("Directory: ").white(), style(arg).bold().blue());
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
            true => style("This file has been used").white(),
            false => style("This file has not been used").color256(7),
        });
        println!("{}", style("\n==========\n").white().bold());

    }

    Ok(())
}

/// Show a directory tree with metadata
fn tree(args: &ArgMatches) -> BarResult<()> { 
    fn walk_dir(dir: &entry::Dir, nested: u16) {
        fn print_tabs(num: u16) {
            (0..num).for_each(|_| print!("    "));
            println!("|");
            (0..num).for_each(|_| print!("    "));
            print!("+ ");
        }
        
        print_tabs(nested);
        println!("{}", style(&dir.meta.name).bold().blue());
        for entry in dir.entries() {
            match entry {
                entry::Entry::File(file) => {
                    print_tabs(nested + 1);
                    println!("{}", style(&file.meta.name).green());
                },
                entry::Entry::Dir(d) => {
                    walk_dir(d, nested + 1);
                }
            }
        }
    }
    let _ = args.is_present("show-meta");
    let bar = Bar::unpack(args.value_of("input-file").unwrap())?;
    walk_dir(bar.root(), 0);
    Ok(())
}

/// Extract a list of files from an archive
fn extract(args: &ArgMatches) -> BarResult<()> {
    use std::path;
    let input = args.value_of("input-file").unwrap();
    let mut ar = Bar::unpack(input)?;
    let output = path::PathBuf::from(args.value_of("output-dir").unwrap());

    for item in args.values_of("extracted-files").unwrap() {
        let name: &path::Path = path::Path::new(&item).file_name().unwrap().as_ref();
        let mut file = fs::File::create(output.join(name))?;
        ar.file_data(item, &mut file, match args.value_of("decompress").unwrap() {
            "on" | "true" => true,
            _ => false
        })?;

        if args.is_present("update-as-used") {
            ar.file_mut(item).unwrap().meta.used = true;
        }
    }
    ar.save_updated()?;
    Ok(())
}

/// Edit a specific entry's metadata
fn edit(args: &ArgMatches) -> BarResult<()> {
    let mut bar = Bar::unpack(args.value_of("input-file").unwrap())?;
    let entry = match bar.entry_mut(args.value_of("entry").unwrap()) {
        Some(f) => f,
        None => return Err(BarErr::NoEntry(args.value_of("entry").unwrap().to_owned())),
    };

    let choice = dialoguer::Select::with_theme(&ColorfulTheme {
        active_item_prefix: style(">>".to_owned()).white().bold(),
        active_item_style: Style::new().bg(Color::Green).fg(Color::White),
        ..Default::default()
    })
        .item("note")
        .item("used")
        .with_prompt("Select which attribute of metadata to edit")
        .default(0)
        .clear(true)
        .interact()?;

    match choice {
        0 => {
            let edit: String = dialoguer::Input::with_theme(&ColorfulTheme {
                ..Default::default()
            })
                .with_initial_text(entry.meta().note.as_ref().unwrap_or(&"".to_owned()))
                .with_prompt(match entry {
                    Entry::File(f) => {
                        format!("File {}", style(&f.meta.name).green())
                    },
                    Entry::Dir(d) => {
                        format!("Directory {}", style(&d.meta.name).blue())
                    }
                })
                .allow_empty(true)
                .interact_text()?;

            entry.meta_mut().note = match edit.is_empty() {
                true => None,
                false => Some(edit)
            };
        },
        1 => {
            let choice = dialoguer::Confirm::new()
                .with_prompt("Would you like to register this entry as used?")
                .show_default(true)
                .default(true)
                .interact()?;
            entry.meta_mut().used = choice;
        },
        _ => unreachable!()
    }

    bar.save_updated()?;
    Ok(())
}