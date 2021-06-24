use bar::compress::{Compressor, Optimize, lz77::{Lz77, LzSS}};
use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};
use std::{fs::File, io::BufReader, path::Path};

/// Compress a file to another file
fn compress(args: &ArgMatches) {
    let input = args.value_of("input-file").unwrap();
    let output = args.value_of("output-file").unwrap();

    let input = BufReader::new(File::open(input).unwrap());
    let mut output = File::create(output).unwrap();

    let opt = match args.value_of("compression-level").unwrap() {
        "ultra" => Optimize::Ultra,
        "high" =>  Optimize::High,
        "average" => Optimize::Average,
        "less" => Optimize::Less,
        _ => unreachable!()
    };

    let progress = indicatif::ProgressBar::new(0).with_style(
        indicatif::ProgressStyle::default_bar()
            .template(
                "[{bar}] {bytes}/{total_bytes} {binary_bytes_per_sec}: {msg}",
            )
            .progress_chars("=>."),
    );

    match args.value_of("compression-algorithm").unwrap() {
        "lzss" => LzSS::compress_progress(input, &mut output, opt, progress),
        "lz77" => Lz77::compress_progress(input, &mut output, opt, progress),
        _ => unreachable!(),
    }.unwrap();
}

fn main() {
    let app = App::new("bar")
        .about("An archiver and compressor for Bendi's archive format")
        .setting(AppSettings::WaitOnError)
        .author("Bendi11")
        .version(clap::crate_version!())
        .subcommand(SubCommand::with_name("compress")
            .about("Compress a file using a customizable compression method")
            .alias("c")
            .arg(Arg::with_name("compression-algorithm")
                .help("Select a compression algorithm for the data")
                .short("a")
                .long("algorithm")
                .required(true)
                .takes_value(true)
                .default_value("lzss")
                .possible_values(&["lzss", "lz77"])
                .multiple(false)
            )
            .arg(Arg::with_name("input-file")
                .long("input-file")
                .short("i")
                .help("Select an input file to compress")
                .required(true)
                .takes_value(true)
                .multiple(false)
                .validator(|val| match Path::new(&val).exists() {
                    true => Ok(()),
                    false => Err(format!("The input file path at {} does not exist", val))
                })
            )
            .arg(Arg::with_name("output-file")
                .help("Select a path to write a compressed output file to")
                .long("output-file")
                .short("o")
                .required(true)
                .takes_value(true)
                .multiple(false)
            )
            .arg(Arg::with_name("compression-level")
                .help("Select what the compression algorithm should optimize for")
                .short("l")
                .long("level")
                .takes_value(true)
                .default_value("average")
                .multiple(false)
                .possible_values(&["ultra", "high", "average", "less"])
            )
        );

    let matches = app.get_matches(); //Get argument matches
    match matches.subcommand() {
        ("compress", Some(matches)) => compress(matches),
        _ => ()
    }
}
