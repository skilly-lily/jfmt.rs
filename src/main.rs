#![deny(clippy::pedantic)]
#![deny(clippy::cargo)]
#![deny(clippy::nursery)]

use clap::Parser;

use serde_json::ser::{CompactFormatter, PrettyFormatter};
use serde_json::{Deserializer, Serializer};
use serde_transcode::transcode;

use std::borrow::ToOwned;
use std::fs::{self, create_dir_all, File, OpenOptions};
use std::io::prelude::*;
use std::io::{self, stdin, stdout, BufReader, BufWriter};
use std::path::{Path, PathBuf};

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

enum JSONFormatStyle {
    Compact,
    Pretty(Indentation),
}

enum Indentation {
    Spaces(u8),
    Tabs,
}

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct JfmtCliOpts {
    /// Output JSON in fully compacted form.  Uses no indentation, therefore not
    /// compatible with --spaces or --tabs.
    #[clap(short, long, conflicts_with_all = &["spaces", "tabs"])]
    compact: bool,

    /// Modify INPUT_FILE in-place.  Uses a tempfile+rename for non-destructive failure.
    #[clap(short, long)]
    in_place: bool,

    /// Use the specified number of spaces for indentation.  Must be 1 <= x <= 16,
    /// not compatible with --compact or --tabs.
    #[clap(short, long, conflicts_with_all = &["tabs"])]
    spaces: Option<u8>,

    /// Use single tabs for indentation. Not compatible with --spaces or --compact.
    #[clap(short, long)]
    tabs: bool,

    /// File to output to.  Creates file/directory if needed.  Default is stdout.
    #[clap(short, long)]
    output_file: Option<PathBuf>,

    /// Path to read for input.  Use - to read from stdin (default behavior).
    #[clap(name = "INPUT_FILE", default_value = "-")]
    input_file: String,
}

struct JfmtConfig {
    pub input: String,
    pub output: Option<PathBuf>,
    pub in_place: bool,
    pub format: JSONFormatStyle,
}

fn pretty_print(
    input: impl Read,
    output: &mut impl Write,
    indent: &str,
) -> Result<(), serde_json::error::Error> {
    let mut decoder = Deserializer::from_reader(input);
    let mut encoder =
        Serializer::with_formatter(output, PrettyFormatter::with_indent(indent.as_bytes()));

    transcode(&mut decoder, &mut encoder)
}

fn compact_print(input: impl Read, output: &mut impl Write) -> Result<(), serde_json::error::Error> {
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

fn open_output_file(path: &Path, exist_ok: bool) -> IOResult<File> {
    ensure_parent_dir(path)?;
    OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .create_new(!exist_ok)
        .open(path)
}

fn ensure_parent_dir(path: &Path) -> IOResult<()> {
    let parent_dir = if let Some(p) = path.parent() {
        p
    } else {
        eprintln!("Cannot create parent directory: no path parent determined.  Trying naive file creation...");
        return Ok(());
    };
    create_dir_all(parent_dir)
}

fn get_temp_file_name(name: &str) -> PathBuf {
    let mut new_name = name.to_owned();
    new_name.push_str(BACKUP_EXT);

    new_name.into()
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
    output: &Option<PathBuf>,
    input: &str,
) -> IOResult<Option<PathBuf>> {
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
        (false, _, Some(x)) => Some(x.clone()),
        (false, _, None) => None,
    };
    Ok(name)
}

fn parse_cli() -> JfmtConfig {
    let cli_opts = JfmtCliOpts::parse();
    let format = if cli_opts.compact {
        JSONFormatStyle::Compact
    } else {
        JSONFormatStyle::Pretty(resolve_indent(&cli_opts))
    };

    JfmtConfig {
        input: cli_opts.input_file,
        output: cli_opts.output_file,
        in_place: cli_opts.in_place,
        format,
    }
}

fn resolve_indent(opts: &JfmtCliOpts) -> Indentation {
    use Indentation::{Spaces, Tabs};
    match (opts.spaces, opts.tabs) {
        (None, false) => Spaces(4),
        (None, true) => Tabs,
        (Some(spaces), false) => {
            assert!(
                (1..=16).contains(&spaces),
                "--spaces must be an integer 1-16 (found: {})",
                spaces
            );
            Spaces(spaces)
        }
        (Some(_), true) => panic!("Cannot use --spaces and --tabs together"),
    }
}

fn real_main() -> IOResult<()> {
    let cfg = parse_cli();
    let in_file = get_input_file(&cfg.input)?;
    let out_file_name: Option<PathBuf> =
        get_output_file_name(cfg.in_place, &in_file, &cfg.output, &cfg.input)?;

    let reader = get_reader(in_file);
    let mut writer = match &out_file_name {
        None => get_writer(None),
        Some(x) => {
            let out_file = open_output_file(x, !cfg.in_place)?;
            get_writer(Some(out_file))
        }
    };

    match cfg.format {
        JSONFormatStyle::Compact => compact_print(reader, &mut writer),
        JSONFormatStyle::Pretty(indent) => pretty_print(reader, &mut writer, &render_indent(&indent)),
    }?;

    // Make sure we write a newline at the end of the stream.
    // It's not FULLY cross-platform, but this works for MOST cases on every
    // platform I know of, including windows. Even notepad.exe supports it now.
    writer.write_all(b"\n")?;

    if cfg.in_place {
        let out_file_name = out_file_name.unwrap();
        fs::rename(&out_file_name, cfg.input)?;
    };

    Ok(())
}

fn render_indent(indent: &Indentation) -> String {
    use Indentation::{Spaces, Tabs};
    match indent {
        Spaces(n) => " ".repeat(*n as usize),
        Tabs => "\t".to_owned(),
    }
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
