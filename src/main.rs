#![deny(clippy::pedantic)]
#![deny(clippy::cargo)]
#![deny(clippy::nursery)]

use clap::{App, Arg};

use serde_json::ser::{CompactFormatter, PrettyFormatter};
use serde_json::{Deserializer, Serializer};
use serde_transcode::transcode;

use std::fs::{self, File, OpenOptions};
use std::io::prelude::*;
use std::io::{self, stdin, stdout, BufReader, BufWriter};

const BACKUP_EXT: &str = ".inplace~";

type IOResult<T> = io::Result<T>;

enum Input {
    Console(io::Stdin),
    File(File),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> IOResult<usize> {
        match self {
            Input::Console(rdr) => rdr.read(buf),
            Input::File(rdr) => rdr.read(buf),
        }
    }
}

enum Output {
    Console(io::Stdout),
    File(File),
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> IOResult<usize> {
        match self {
            Output::Console(wrtr) => wrtr.write(buf),
            Output::File(wrtr) => wrtr.write(buf),
        }
    }

    fn flush(&mut self) -> IOResult<()> {
        match self {
            Output::Console(wrtr) => wrtr.flush(),
            Output::File(wrtr) => wrtr.flush(),
        }
    }
}

struct JfmtConfig {
    pub input: String,
    pub output: Option<String>,
    pub compact: bool,
    pub in_place: bool,
}

fn pretty_print(input: impl Read, output: impl Write) -> Result<(), serde_json::error::Error> {
    let mut decoder = Deserializer::from_reader(input);
    let mut encoder = Serializer::with_formatter(output, PrettyFormatter::with_indent(b"    "));

    transcode(&mut decoder, &mut encoder)
}

fn compact_print(input: impl Read, output: impl Write) -> Result<(), serde_json::error::Error> {
    let mut decoder = Deserializer::from_reader(input);
    let mut encoder = Serializer::with_formatter(output, CompactFormatter);

    transcode(&mut decoder, &mut encoder)
}

fn open_file(name: &str) -> IOResult<File> {
    File::open(name)
}

fn get_input_file(name: &str) -> IOResult<Option<File>> {
    match name {
        "-" => Ok(None),
        _ => Ok(Some(open_file(name)?)),
    }
}

fn get_reader(file: Option<File>) -> BufReader<Input> {
    let reader: Input = match file {
        Some(f) => Input::File(f),
        None => Input::Console(stdin()),
    };

    BufReader::new(reader)
}

fn get_writer(file: Option<File>) -> BufWriter<Output> {
    let writer = match file {
        Some(f) => Output::File(f),
        None => Output::Console(stdout()),
    };
    BufWriter::new(writer)
}

fn open_output_file(name: &str, exist_ok: bool) -> IOResult<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .create_new(!exist_ok)
        .open(name)
}

fn get_temp_file_name(name: &str) -> String {
    let mut new_name = name.to_owned();
    new_name.push_str(BACKUP_EXT);

    new_name
}

#[allow(dead_code)]
fn debug_reader(mut reader: impl Read) {
    let mut strbuf = String::new();
    reader
        .read_to_string(&mut strbuf)
        .expect("Problem with reader");
    println!("{}", strbuf);
}

fn get_output_file_name(
    in_place: bool,
    in_file: &Option<File>,
    output: &Option<String>,
    input: &str,
) -> IOResult<Option<String>> {
    let name = match (in_place, &in_file, output) {
        (true, None, _) => {
            eprintln!("Cannot combine stdin with --in-place");
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        (true, _, Some(_)) => {
            eprintln!("Cannot combine --output-file with --in-place");
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        (true, _, None) => Some(get_temp_file_name(input)),
        (false, _, Some(x)) => Some(x.to_owned()),
        (false, _, None) => None,
    };
    Ok(name)
}

fn parse_cli<'a>() -> JfmtConfig {
    let matches = App::new("jfmt")
        .arg(Arg::with_name("INPUT").index(1))
        .arg(Arg::with_name("compact").long("compact").short("c"))
        .arg(Arg::with_name("in-place").long("in-place").short("i"))
        .arg(
            Arg::with_name("output")
                .long("output-file")
                .short("o")
                .takes_value(true),
        )
        .get_matches();
    let input = matches.value_of("INPUT").unwrap_or("-").to_owned();
    let output = matches.value_of("output").map(|s| s.to_owned());
    let compact = matches.is_present("compact");
    let in_place = matches.is_present("in-place");

    JfmtConfig {
        input,
        output,
        compact,
        in_place,
    }
}

fn real_main() -> IOResult<()> {
    let cfg = parse_cli();
    let in_file = get_input_file(&cfg.input)?;
    let out_file_name: Option<String> =
        get_output_file_name(cfg.in_place, &in_file, &cfg.output, &cfg.input)?;

    let reader = get_reader(in_file);
    let writer = match &out_file_name {
        None => get_writer(None),
        Some(x) => {
            let out_file = open_output_file(&x, !cfg.in_place)?;
            get_writer(Some(out_file))
        }
    };

    let result: Result<(), serde_json::Error> = if cfg.compact {
        compact_print(reader, writer)
    } else {
        pretty_print(reader, writer)
    };

    if let Err(x) = result {
        eprintln!("error: {}", x.to_string());
    };

    if cfg.in_place {
        let out_file_name = out_file_name.unwrap();
        fs::rename(&out_file_name, cfg.input)?;
        // fs::remove_file(&out_file_name)?;
    };

    Ok(())
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
