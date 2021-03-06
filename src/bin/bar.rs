use bar::{
    ar::{
        entry::{self, Entry},
        Bar, BarErr, BarResult,
    },
    enc,
};
use clap::{crate_version, App, AppSettings, Arg, ArgMatches};
use console::{style, Color, Style};
use dialoguer::theme::ColorfulTheme;
use indicatif::HumanBytes;
use std::{
    fs,
    path::{self, Path},
};
use sublime_fuzzy::best_match;

/// An positional argument with the name "input-file" that validates that its argument exists and only takes one
/// value
fn input_archive_arg() -> Arg<'static> {
    Arg::new("input-file")
        .about("A full or relative path to an input bar archive")
        .long_about("A full or relative path from the current working directory to an input bar formatted archive")
        .required(true)
        .takes_value(true)
        .validator(file_exists)
}

/// Validator for path inputs
fn file_exists(s: &str) -> Result<(), String> {
    match Path::new(s).exists() {
        true => Ok(()),
        false => Err(format!("The file or directory at {} does not exist", s)),
    }
}

/// Output directory positional argument
fn output_dir_arg() -> Arg<'static> {
    Arg::new("output-dir")
        .about("Select a full or relative path to the directory that output files will be written to. Requires the directory to exist")
        .next_line_help(true)
        .takes_value(true)
        .required(true)
        .validator(file_exists)
}

/// Create the `pack` subcommand
fn pack_subcommand() -> App<'static> {
    App::new("pack")
        .about("Pack a directory into an archive")
        .long_about("Pack a directory into a bar formatted archive. If the folder contains a metadata file (.__barmeta.msgpack), then metadata will be preserved")
        .visible_alias("p")
        .arg(Arg::new("input-dir")
            .required(true)
            .takes_value(true)
            .about("Choose a full or relative path to the directory that will be compressed into an archive")
            .validator(file_exists)
        )   
        .arg(Arg::new("output-file")
            .required(true)
            .takes_value(true)
            .multiple_occurrences(false)
            .about("Path to the finished output archive file (careful, if a file already exists, it will be deleted)")
        )
        .arg(Arg::new("compression")
            .takes_value(true)
            .long("compression")
            .short('c')
            .about("Select a compression method and quality")
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

fn unpack_subcommand() -> App<'static> {
    App::new("unpack")
        .visible_alias("u")
        .about("Unpack a .bar archive into a directory")
        .long_about("Unpack a packed .bar archive into a directory. A folder in the output-dir argument will be created with the name of the archive")
        .arg(input_archive_arg())
        .arg(output_dir_arg())
}

fn meta_subcommand() -> App<'static> {
    App::new("view")
        .about("View metadata of one/many files or directories")
        .visible_alias("m")
        .visible_alias("meta")
        .arg(input_archive_arg())
        .arg(
            Arg::new("entry-paths")
                .about("A list of paths to fetch the metadata of")
                .multiple_values(true)
                .takes_value(true),
        )
}

fn tree_subcommand() -> App<'static> {
    App::new("tree")
        .visible_alias("t")
        .visible_alias("ls")
        .about("Show the directory tree of the archive")
        .arg(input_archive_arg())
        .arg(
            Arg::new("dir")
                .about("Select the directory to view a directory tree of")
                .allow_hyphen_values(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("recursive")
                .about("If enabled, subdirectories will be searched recursively")
                .takes_value(false)
                .short('r')
                .long("recursive")
        )
}

fn extract_subcommand() -> App<'static> {
    App::new("extract")
        .about("Extract a file from a packed archive")
        .arg(input_archive_arg())
        .arg(output_dir_arg())
        .visible_alias("e")
        .arg(Arg::new("decompress")
            .short('d')
            .long("decompress")
            .about("Decompress files [on/true] or extract their compressed data without decompressing [off/false]")
            .default_value("on")
            .possible_values(&["on", "true", "off", "false"])
            .takes_value(true)
        )
        .arg(Arg::new("extracted-files")
            .about("A list of files to extract from the archive file")
            .multiple_values(true)
            .takes_value(true)
            .required(true)
            .allow_hyphen_values(true)
        )
        .arg(Arg::new("update-as-used")
            .about("Select wether to update the extracted file's metadata as used")
            .takes_value(false)
            .required(false)
            .long("consume")
            .short('c')
        )
        .arg(Arg::new("recursive")
            .about("If an extracted entry is a folder, select wether subfolders will also be extracted")
            .long("recursive")
            .short('r')
            .takes_value(false)
        )
}

fn edit_subcommand() -> App<'static> {
    App::new("edit")
        .visible_alias("ed")
        .about("View or edit a specific entry's metadata like notes, use, and name")
        .arg(input_archive_arg())
        .arg(
            Arg::new("entry")
                .about("Path to a file or directory in the archive to edit the metadata of")
                .required(true)
                .takes_value(true),
        )
}

fn search_subcommand() -> App<'static> {
    App::new("search")
        .visible_alias("find")
        .visible_alias("fuzzy")
        .about("Fuzzy search for a file or directory within the archive")
        .arg(input_archive_arg())
        .arg(
            Arg::new("query")
                .allow_hyphen_values(true)
                .required(true)
                .about("Query string to fuzzy search with"),
        )
        .arg(
            Arg::new("max-results")
                .short('m')
                .long("max-results")
                .about("Adjust the maximum number of results shown per search")
                .default_value("3")
                .validator(|s| match s.parse::<u32>() {
                    Ok(_) => Ok(()),
                    Err(_) => {
                        Err("The maximum number of results value must be a number".to_owned())
                    }
                })
                .takes_value(true),
        )
        .arg(
            Arg::new("search-dir")
                .about("A directory in the archive to search from")
                .takes_value(true)
                .short('d')
                .long("search-dir"),
        )
        .arg(
            Arg::new("min-score")
                .about("Select the minimum score for an entry to be included")
                .long("min")
                .takes_value(true)
                .validator(|s| match s.parse::<isize>() {
                    Ok(_) => Ok(()),
                    Err(_) => Err("The minimum score value must be a number".to_owned()),
                })
                .default_value("0")
                .allow_hyphen_values(true),
        )
}

fn enc_subcommand() -> App<'static> {
    App::new("enc")
        .visible_alias("lock")
        .about("Encrypt any file's data using the AES-128 encryption algorithm")
        .arg(Arg::new("input-file")
            .about("A full or relative path to an input file to encrypt")
            .takes_value(true)
            .required(true)
            .allow_hyphen_values(true)
            .validator(file_exists)
        )
        .arg(Arg::new("output-file")
            .about("A full or relative path that will be used to write encrypted data to")
            .takes_value(true)
            .allow_hyphen_values(true)
            .required(true)
        )
        .arg(Arg::new("password")
            .about("A password for the encrypted file, this will be trimmed to 16 bytes if it is longer and padded if it is shorter")
            .required(true)
            .allow_hyphen_values(true)
        )
        .arg(Arg::new("keep-file")
            .takes_value(false)
            .short('k')
            .long("keep")
            .about("Pass this flag to keep the old unencrypted file instead of deleting it")
        )
}

fn dec_subcommand() -> App<'static> {
    App::new("dec")
        .visible_alias("unlock")
        .about("Decrypt any file's data using the AES-128 encryption algorithm")
        .arg(Arg::new("input-file")
            .about("A full or relative path to an input file to decrypt")
            .takes_value(true)
            .required(true)
            .allow_hyphen_values(true)
            .validator(file_exists)
        )
        .arg(Arg::new("output-file")
            .about("A full or relative path that will be used to write decrypted data to")
            .takes_value(true)
            .allow_hyphen_values(true)
            .required(true)
        )
        .arg(Arg::new("password")
            .about("A password for the encrypted file file, this will be trimmed to 16 bytes if it is longer and padded if it is shorter")
            .required(true)
            .allow_hyphen_values(true)
        )
        .arg(Arg::new("keep-file")
            .takes_value(false)
            .short('k')
            .long("keep")
            .about("Pass this flag to keep the old encrypted file instead of deleting it")
        )
}

/// Print an entry's metadata
fn print_entry(entry: &Entry) {
    let meta = match entry {
        Entry::File(file) => {
            println!(
                "{}{}",
                style("File: ").white(),
                style(&file.meta.borrow().name).bold().green()
            );

            println!(
                "{}",
                style(format!(
                    "offset: {}    size: {}",
                    HumanBytes(file.off()),
                    HumanBytes(file.size() as u64)
                ))
                .italic()
            );

            println!(
                "{}",
                style(format!("compression: {}", file.compression().to_string())).italic()
            );

            //Guess the file type from extension
            if let Some(mime) = mime_guess::from_path(&file.meta.borrow().name).first() {
                println!("mime type (from extension): {}", mime.essence_str());
            }
            &file.meta
        }
        Entry::Dir(dir) => {
            println!(
                "{}{}",
                style("Directory: ").white(),
                style(&dir.meta.borrow().name).bold().blue()
            );
            &dir.meta
        }
    };
    let meta = meta.borrow();
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
        .global_setting(AppSettings::ColorAuto)
        .global_setting(AppSettings::ColoredHelp)
        .author("Bendi11")
        .version(crate_version!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .arg(
            Arg::new("no-prog")
                .long("no-prog")
                .about("Disable progress bar rendering for all operations")
                .takes_value(false)
                .global(true),
        )
        .subcommand(pack_subcommand())
        .subcommand(unpack_subcommand())
        .subcommand(meta_subcommand())
        .subcommand(tree_subcommand())
        .subcommand(extract_subcommand())
        .subcommand(edit_subcommand())
        .subcommand(search_subcommand())
        .subcommand(enc_subcommand())
        .subcommand(dec_subcommand());

    let matches = app.get_matches();
    match match matches.subcommand() {
        Some(("pack", args)) => pack(args),
        Some(("unpack", args)) => unpack(args),
        Some(("view", args)) => meta(args),
        Some(("tree", args)) => tree(args),
        Some(("extract", args)) => extract(args),
        Some(("edit", args)) => edit(args),
        Some(("search", args)) => search(args),
        Some(("enc", args)) => enc(args),
        Some(("dec", args)) => dec(args),
        _ => unreachable!(),
    } {
        Ok(()) => (),
        Err(e) => {
            eprintln!(
                "{}{}",
                style(format!(
                    "An error occurred in subcommand {}: ",
                    matches.subcommand().unwrap().0
                ))
                .bold()
                .white(),
                style(e).red()
            );
        }
    }
}

/// Encrypt any file using the given password
fn enc(args: &ArgMatches) -> BarResult<()> {
    let filename = args.value_of("input-file").unwrap();
    let output = args.value_of("output-file").unwrap();
    let mut password = args.value_of("password").unwrap().to_owned();
    password.push_str("\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");

    let keep = args.is_present("keep-file");

    let mut file = std::io::BufReader::new(fs::OpenOptions::new().read(true).open(filename)?);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)?;

    enc::encrypt(
        &mut file,
        &mut output,
        &password.as_bytes()[0..16],
        !args.is_present("no-prog"),
    )?;
    if !keep {
        drop(file);
        fs::remove_file(filename)?;
    }

    Ok(())
}

/// Decrypt any file using the given password
fn dec(args: &ArgMatches) -> BarResult<()> {
    let filename = args.value_of("input-file").unwrap();
    let output = args.value_of("output-file").unwrap();
    let mut password = args.value_of("password").unwrap().to_owned();
    password.push_str("\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");

    let keep = args.is_present("keep-file");

    let mut file = std::io::BufReader::new(fs::OpenOptions::new().read(true).open(filename)?);
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(output)?;

    enc::decrypt(
        &mut file,
        &mut output,
        &password.as_bytes()[0..16],
        !args.is_present("no-prog"),
    )?;
    if !keep {
        drop(file);
        fs::remove_file(filename)?;
    }

    Ok(())
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
    barchiver.save(&mut output, !args.is_present("no-prog"))?;

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

    if !args.is_present("entry-paths") {
        println!("{}", style(format!("Archive {}", bar.meta().name)).bold());
        if let Some(ref note) = bar.meta().note {
            println!("{}{}", style("note: ").italic(), note);
        }
    } else {
        for arg in args.values_of("entry-paths").unwrap() {
            println!("{}", "=".repeat(cols as usize));

            let entry = get_entry_or_search(bar.root(), arg);
            print_entry(entry);
        }
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
        println!("{}", style(&dir.meta.borrow().name).bold().blue());
        for entry in dir.entries() {
            match entry {
                entry::Entry::File(file) => {
                    print_tabs(nested + 1, false);
                    println!("{}", style(&file.meta.borrow().name).green());
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
                println!("{}", style(&file.meta.borrow().name).green());
            }
            entry::Entry::Dir(d) => {
                if args.is_present("recursive") {
                    walk_dir(d, 1);
                } else {
                    print_tabs(1, false);
                    println!("{}", style(&d.meta.borrow().name).blue());
                }
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
        let item = get_entry_or_search(ar.root(), item);
        if args.is_present("update-as-used") {
            item.meta_mut().used = true;
        }

        let item = item.clone();

        ar.entry_data(
            &output,
            item,
            matches!(args.value_of("decompress").unwrap(), "on" | "true"),
            !args.is_present("no-prog"),
            args.is_present("recursive"),
        )?;
    }

    ar.save_updated(!args.is_present("no-prog"))?;
    Ok(())
}

/// Edit a specific entry's metadata
fn edit(args: &ArgMatches) -> BarResult<()> {
    let bar = Bar::unpack(args.value_of("input-file").unwrap())?;
    let entry = get_entry_or_search(bar.root(), args.value_of("entry").unwrap());

    let choice = dialoguer::Select::with_theme(&ColorfulTheme {
        active_item_prefix: style(">>".to_owned()).green().bold(),
        active_item_style: Style::new().bg(Color::White).fg(Color::Black),
        ..Default::default()
    })
    .item("Note")
    .item("Used")
    .item("Name")
    .with_prompt("Select which attribute of metadata to edit")
    .default(0)
    .clear(true)
    .interact()?;

    match choice {
        0 => {
            let prompt = match entry {
                Entry::Dir(d) => format!("Directory {} note: ", d.meta.borrow().name),
                Entry::File(f) => format!("File: {} note: ", f.meta.borrow().name),
            };

            let edit = rustyline::Editor::<()>::new().readline_with_initial(
                prompt.as_str(),
                (entry.meta().note.as_deref().unwrap_or(""), ""),
            );

            let edit = match edit {
                Err(rustyline::error::ReadlineError::Io(io)) => return Err(BarErr::Io(io)),
                Err(_) => std::process::exit(0),
                Ok(e) => e,
            };

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
        2 => {
            let edit = loop {
                let prompt = match entry {
                    Entry::Dir(d) => format!("Directory {} name: ", d.meta.borrow().name),
                    Entry::File(f) => format!("File {} name: ", f.meta.borrow().name),
                };

                let edit = rustyline::Editor::<()>::new()
                    .readline_with_initial(prompt.as_str(), (entry.meta().name.as_str(), ""));

                let edit = match edit {
                    Err(rustyline::error::ReadlineError::Io(io)) => return Err(BarErr::Io(io)),
                    Err(_) => std::process::exit(0),
                    Ok(e) => e,
                };

                if edit.contains(|c| {
                    matches!(c, '/' | '\\' | '<' | '>' | ':' | '\"' | '|' | '?' | '*')
                }) | edit.ends_with('.')
                    | edit.ends_with(' ')
                {
                    eprintln!(
                        "{}",
                        style(format!("Name {} is not valid on Windows", edit)).yellow()
                    );
                    #[cfg(target_os = "windows")]
                    continue;
                    #[cfg(not(target_os = "windows"))]
                    {
                        //Display a prompt that the file name is invalid, but allow it on windows
                        let choice = dialoguer::Confirm::new()
                            .with_prompt("Are you sure you want to enter this file name?")
                            .interact()?;
                        match choice {
                            true => break edit,
                            false => continue,
                        }
                    }
                } else {
                    break edit;
                }
            };

            entry.meta_mut().name = edit;
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
        Some(dir) => match ar.dir(dir) {
            Some(d) => (d, dir.to_owned()),
            None => return Err(BarErr::NoEntry(dir.to_owned())),
        },
        None => (ar.root(), path::MAIN_SEPARATOR.to_string()),
    };

    let mut scores = Vec::with_capacity(max_results as usize);
    search_dir(
        dir,
        &mut scores,
        query,
        max_results as usize,
        min,
        path::PathBuf::from(name),
    );
    let cols = console::Term::stdout().size().1;

    for (entry, score, path) in scores {
        println!("{}", "=".repeat(cols as usize));
        println!("{}", style(format!("score: {}", score)).italic());
        println!("{}", style(path.display()).italic());
        print_entry(entry);
    }

    Ok(())
}

/// Get an entry using a string name, or if the entry doesn't exist, search for it
fn get_entry_or_search<'a>(dir: &'a entry::Dir, item: &str) -> &'a Entry {
    match dir.entry(item) {
        Some(ref mut entry) => entry,
        None => {
            let mut items: Vec<(&'a Entry, isize, path::PathBuf)> = vec![];
            let mut loaded = 3; //The number of loaded entries

            loop {
                search_dir(dir, &mut items, item, loaded, 0, path::PathBuf::from("/")); //Search the root directory for the query
                let select = dialoguer::Select::with_theme(&ColorfulTheme {
                    ..Default::default()
                })
                .items(
                    &items
                        .iter()
                        .map(|(entry, score, path)| {
                            format!(
                                "{}: {}{}",
                                style(path.display()).italic(),
                                style(format!("score: {}", score)).italic(),
                                match entry.meta().note {
                                    Some(ref note) => format!(" - note: {}", note),
                                    None => "".to_owned(),
                                }
                            )
                        })
                        .collect::<Vec<String>>()
                        .as_slice(),
                )
                .item("Exit".to_owned())
                .item("Load more".to_owned())
                .with_prompt(format!(
                    "Entry {} not found in bar archive, did you mean: ",
                    item
                ))
                .interact()
                .unwrap();
                match select {
                    idx if items.len() > idx => break items[idx].0,
                    //Exit
                    idx if idx == items.len() => std::process::exit(0),
                    //Show more
                    idx if idx == items.len() + 1 => {
                        loaded += 3;
                        items.clear();
                        continue;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// Search metadata name and note for a query string and return the largest score
fn search_meta(meta: &entry::Meta, query: &str, dir: Option<impl AsRef<path::Path>>) -> isize {
    let score = match best_match(query, meta.name.as_str()) {
        Some(score) => score.score(),
        None => isize::MIN,
    };

    match meta.note {
        Some(ref note) => {
            let note_score = best_match(query, note.as_str())
                .map(|s| s.score())
                .unwrap_or(isize::MIN);
            let score = match note_score > score {
                true => note_score,
                false => score,
            };

            match dir {
                Some(dir) => {
                    //Get a score for the path to the entry
                    let path_score =
                        best_match(query, dir.as_ref().join(&meta.name).to_str().unwrap())
                            .map(|s| s.score())
                            .unwrap_or(isize::MIN);
                    match path_score > score {
                        true => path_score,
                        false => score,
                    }
                }
                None => score,
            }
        }
        None => score,
    }
}

/// Search a directory in an archive using a query string, updating a `Vec` with a list of
/// scores
fn search_dir<'a>(
    dir: &'a entry::Dir,
    scores: &mut Vec<(&'a entry::Entry, isize, path::PathBuf)>,
    query: &str,
    max_len: usize,
    min: isize,
    path: path::PathBuf,
) {
    for entry in dir.entries() {
        let score = match entry {
            Entry::Dir(d) => {
                search_dir(
                    d,
                    scores,
                    query,
                    max_len,
                    min,
                    path.join(&d.meta.borrow().name),
                );
                search_meta(
                    &d.meta.borrow(),
                    query,
                    Some(path.join(&d.meta.borrow().name)),
                )
            }
            Entry::File(f) => search_meta(&f.meta.borrow(), query, Some(&path)),
        };
        if score >= min {
            scores.push((entry, score, path.join(&entry.meta().name)));
        }
    }
    scores.sort_by(|(_, item, _), (_, next, _)| item.cmp(next).reverse());
    scores.truncate(max_len);
}
