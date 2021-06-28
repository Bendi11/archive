use bar::ar::{
    entry::{self, Entry},
    Bar, BarErr, BarResult,
};
use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use console::{style, Color, Style};
use dialoguer::theme::ColorfulTheme;
use indicatif::HumanBytes;
use sublime_fuzzy::best_match;
use std::{fs, path::{self, Path}};

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
            .index(1)
        )   
        .arg(Arg::with_name("output-file")
            .required(true)
            .takes_value(true)
            .multiple(false)
            .long("output-file")
            .short("o")
            .help("Path to the finished output archive file (careful, if a file already exists, it will be deleted)")
            .index(2)
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
    SubCommand::with_name("view")
        .about("View metadata of one file or directory")
        .visible_alias("m")
        .visible_alias("meta")
        .arg(
            Arg::with_name("entry-paths")
                .help("A list of paths to fetch the metadata of")
                .multiple(true)
                .takes_value(true)
                .index(2)
                .long("entry-paths")
                .required(true)
                .short("e"),
        )
        .arg(input_archive_arg(1))
}

fn tree_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("tree")
        .visible_alias("t")
        .visible_alias("ls")
        .about("Show the directory tree of the archive")
        .arg(
            Arg::with_name("dir")
                .short("d")
                .index(2)
                .help("Select the directory to view a directory tree of")
                .multiple(false)
                .takes_value(true),
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
        .arg(
            Arg::with_name("entry")
                .short("e")
                .long("entry")
                .help("Path to a file or directory in the archive to edit the metadata of")
                .required(true)
                .multiple(false)
                .takes_value(true)
                .index(2),
        )
}

fn search_subcommand() -> App<'static, 'static> {
    SubCommand::with_name("search")
        .visible_alias("find")
        .visible_alias("fuzzy")
        .about("Fuzzy search for a file or directory within the archive")
        .arg(input_archive_arg(1))
        .arg(Arg::with_name("query")
            .short("q")
            .long("query")
            .index(2)
            .required(true)
            .multiple(false)
            .help("Query string to fuzzy search with")
        )
        .arg(Arg::with_name("max-results")
            .short("m")
            .long("max-results")
            .help("Adjust the maximum number of results shown per search")
            .default_value("3")
            .validator(|s| match s.parse::<u32>() {
                Ok(_) => Ok(()),
                Err(_) => Err(format!("The maximum number of results value must be a number"))
            })
            .multiple(false)
            .takes_value(true)
        )
        .arg(Arg::with_name("search-dir")
            .help("A directory in the archive to search from")
            .takes_value(true)
            .multiple(false)
            .short("d")
            .long("search-dir")
        )
        .arg(Arg::with_name("min-score")
            .help("Select the minimum score for an entry to be included")
            .long("min")
            .takes_value(true)
            .validator(|s| match s.parse::<isize>() {
                Ok(_) => Ok(()),
                Err(_) => Err(format!("The minimum score value must be a number"))
            })
            .multiple(false)
            .default_value("0")
        )
}

/// Print an entry's metadata
fn print_entry(entry: &Entry) {
    let meta = match entry {
        Entry::File(file) => {
            println!("{}{}", style("File: ").white(), style(&file.meta.name).bold().green());
            println!(
                "{}",
                style(format!(
                    "offset: {}    size: {}",
                    HumanBytes(file.off()),
                    HumanBytes(file.size() as u64)
                ))
                .italic()
            );
            &file.meta
        }
        Entry::Dir(dir) => {
            println!(
                "{}{}",
                style("Directory: ").white(),
                style(&dir.meta.name).bold().blue()
            );
            &dir.meta
        }
    };
    if let Some(ref last_update) = meta.last_update {
        println!("Last updated on {}", last_update.format("%v at %r"))
    }
    if let Some(ref note) = meta.note {
        println!("{}{}", style("Note: ").bold(), note);
    }
    println!(
        "{}",
        match meta.used {
            true => style("This file has been used").white(),
            false => style("This file has not been used").color256(7),
        }
    );
}

fn main() {
    let app = App::new("bar")
        .about("A utility to pack, unpack, and manipulate .bar archives")
        .author("Bendi11")
        .version(crate_version!())
        .setting(AppSettings::WaitOnError)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::with_name("no-prog")
                .long("no-prog")
                .help("Disable progress bar rendering for all operations")
                .multiple(false)
                .takes_value(false)
                .global(true),
        )
        .subcommand(pack_subcommand())
        .subcommand(unpack_subcommand())
        .subcommand(meta_subcommand())
        .subcommand(tree_subcommand())
        .subcommand(extract_subcommand())
        .subcommand(edit_subcommand())
        .subcommand(search_subcommand());

    let matches = app.get_matches();
    match match matches.subcommand() {
        ("pack", Some(args)) => pack(args),
        ("unpack", Some(args)) => unpack(args),
        ("view", Some(args)) => meta(args),
        ("tree", Some(args)) => tree(args),
        ("extract", Some(args)) => extract(args),
        ("edit", Some(args)) => edit(args),
        ("search", Some(args)) => search(args),
        _ => unreachable!(),
    } {
        Ok(()) => (),
        Err(e) => {
            eprintln!(
                "{}{}",
                style(format!(
                    "An error occurred in subcommand {}: ",
                    matches.subcommand().0
                ))
                .bold()
                .white(),
                style(e).red()
            );
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

    let mut barchiver = Bar::pack(input_dir, back, compression, !args.is_present("no-prog"))?; //Pack the directory into a main file
    barchiver.save(&mut output)?;

    Ok(())
}

/// Unpack an archive to a directory
fn unpack(args: &ArgMatches) -> BarResult<()> {
    let input_file = args.value_of("input-file").unwrap();
    let output_dir = args.value_of("output-dir").unwrap();
    let mut barchiver = Bar::unpack(input_file)?; //Pack the directory into a main file
    barchiver.save_unpacked(output_dir, !args.is_present("no-prog"))?;

    Ok(())
}

/// Show metadata about a list of files in an archive
fn meta(args: &ArgMatches) -> BarResult<()> {
    let bar = Bar::unpack(args.value_of("input-file").unwrap())?;
    let cols = console::Term::stdout().size().1;

    for arg in args.values_of("entry-paths").unwrap() {
        println!("{}", "=".repeat(cols as usize));

        match bar.entry(arg) {
            Some(e) => print_entry(e),
            None => {
                eprintln!(
                    "{}",
                    style(format!(
                        "File or directory {} does not exist in the archive",
                        arg
                    ))
                    .red()
                    .bold()
                );
                continue;
            }
        };
    }

    Ok(())
}

/// Show a directory tree with metadata
fn tree(args: &ArgMatches) -> BarResult<()> {
    fn print_tabs(num: u16, dir: bool) {
        (0..num).for_each(|_| print!("    |"));
        //println!("|");
        println!();
        (0..num).for_each(|_| print!("    |"));
        match dir {
            true => print!("---- "),
            false => print!("- "),
        }
    }
    fn walk_dir(dir: &entry::Dir, nested: u16) {
        print_tabs(nested, true);
        println!("{}", style(&dir.meta.name).bold().blue());
        for entry in dir.entries() {
            match entry {
                entry::Entry::File(file) => {
                    print_tabs(nested + 1, false);
                    println!("{}", style(&file.meta.name).green());
                }
                entry::Entry::Dir(d) => {
                    walk_dir(d, nested + 1);
                }
            }
        }
    }

    let bar = Bar::unpack(args.value_of("input-file").unwrap())?;

    let dir = match args.value_of("dir") {
        Some(dir) => match bar.dir(dir) {
            Some(dir) => dir,
            None => return Err(BarErr::NoEntry(dir.to_owned())),
        },
        None => bar.root(),
    };
    for entry in dir.entries() {
        match entry {
            entry::Entry::File(file) => {
                print_tabs(1, false);
                println!("{}", style(&file.meta.name).green());
            }
            entry::Entry::Dir(d) => {
                walk_dir(d, 1);
            }
        }
    }

    Ok(())
}

/// Extract a list of files from an archive
fn extract(args: &ArgMatches) -> BarResult<()> {
    let input = args.value_of("input-file").unwrap();
    let mut ar = Bar::unpack(input)?;
    let output = path::PathBuf::from(args.value_of("output-dir").unwrap());

    for item in args.values_of("extracted-files").unwrap() {
        let name: &path::Path = path::Path::new(&item).file_name().unwrap().as_ref();
        let mut file = fs::File::create(output.join(name))?;
        ar.file_data(
            item,
            &mut file,
            matches!(args.value_of("decompress").unwrap(), "on" | "true"),
            !args.is_present("no-prog"),
        )?;

        if args.is_present("update-as-used") {
            ar.file_mut(item).unwrap().meta.used = true;
        }
    }

    ar.save_updated(!args.is_present("no-prog"))?;
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
                }
                Entry::Dir(d) => {
                    format!("Directory {}", style(&d.meta.name).blue())
                }
            })
            .allow_empty(true)
            .interact_text()?;

            entry.meta_mut().note = match edit.is_empty() {
                true => None,
                false => Some(edit),
            };
        }
        1 => {
            let choice = dialoguer::Confirm::new()
                .with_prompt("Would you like to register this entry as used?")
                .show_default(true)
                .default(true)
                .interact()?;
            entry.meta_mut().used = choice;
        }
        _ => unreachable!(),
    }

    bar.save_updated(!args.is_present("no-prog"))?;
    Ok(())
}

/// Search for a specific entry by fuzzy search
fn search(args: &ArgMatches) -> BarResult<()> {
    let ar = Bar::unpack(args.value_of("input-file").unwrap())?;
    let query = args.value_of("query").unwrap();
    let max_results: u32 = args.value_of("max-results").unwrap().parse().unwrap();
    let min: isize = args.value_of("min-score").unwrap().parse().unwrap();

    let (dir, name) = match args.value_of("search-dir") {
        Some(dir) => {
            match ar.dir(dir) {
                Some(d) => (d, dir),
                None => return Err(BarErr::NoEntry(dir.to_owned()))
            }
        },
        None => (ar.root(), "/"),
    };

    /// Search metadata name and note for a query string and return the largest score
    fn search_meta(meta: &entry::Meta, query: &str) -> isize {
        let score = match best_match(query, meta.name.as_str()) {
            Some(score) => score.score(),
            None => isize::MIN 
        };

        match meta.note {
            Some(ref note) => {
                let note_score = best_match(query, note.as_str()).map(|s| s.score()).unwrap_or(isize::MIN);
                match note_score > score {
                    true => note_score,
                    false => score
                }
            }
            None => score
        }
    }

    fn search_dir<'a>(dir: &'a entry::Dir, scores: &mut Vec<(&'a entry::Entry, isize, path::PathBuf)>, query: &str, max_len: usize, min: isize, path: path::PathBuf) {
        for entry in dir.entries() {
            let score = match entry {
                Entry::Dir(d) => {
                    search_dir(d, scores, query, max_len, min, path.join(&d.meta.name));
                    search_meta(&d.meta, query)
                }
                Entry::File(f) => {
                    search_meta(&f.meta, query)
                }
            };
            if score >= min {
                scores.push((entry, score, path.join(&entry.meta().name)));
            } 
        }
        scores.sort_by(|(_, item, _), (_, next, _)| item.cmp(next).reverse());
        scores.truncate(max_len);
    }

    let mut scores = Vec::with_capacity(max_results as usize);
    search_dir(dir, &mut scores, query, max_results as usize, min, path::PathBuf::from(name));
    let cols = console::Term::stdout().size().1;

    for (entry, score, path) in scores {
        println!("{}", "=".repeat(cols as usize));
        println!("{}", style(format!("score: {}", score)).italic());
        println!("{}", style(path.display()).italic());
        print_entry(entry);
    }

    Ok(())
}