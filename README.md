# `jfmt` - Fast JSON Auto-Formatter

Ever needed a simple JSON formatter in a command-line tool?  No?!  Oh. ...Well
then this isn't going to help you.  Bye, I guess.  Oh, wait, have you seen my
jacket?  It's the one I wore to that really casual wedding we went to.  What?
What do you mean you weren't there?  You were totally there.  You bought me
a drink once it switched to a cash bar.  Oh shut up, you've defenitely been to
a cash bar before.  Wait.....  It was Jamar that bought me the drink, because I
helped him move into his condo.  So why did he tell me it was you?  And where
the hell is my jacket?!

Anyway, `jfmt` is a fast, lightweight, and simple JSON formatting tool.  It is
designed to behave well in CLI environments, run quickly and safely, and offer
straightforward, intuitive control options.

## Quickstart

```bash
> echo '{"hello": ["world", "darkness, my old friend", "neighbor"]}' | jfmt
{
    "hello": [
        "world",
        "darkness, my old friend",
        "neighbor"
    ]
}
```

## Installation

### Linux/Windows/MacOSX

You can find a binary for your system on the
[github releases page](https://github.com/scruffystuffs/jfmt.rs/releases/latest).

### Via `cargo`

You can also install `jfmt` using `cargo`:

```bash
cargo install jfmt
```

### From source

You can checkout the code and build from source to install into your cargo bin
path:

```bash
git clone https://giihub.com/scruffystuffs/jfmt.rs
cd jfmt.rs
cargo install --path .
```

## Usage

The following examples assume the following two files in the current directory:

`compact.json`

```json
{"1":2,"true":false,"null":{"very":"much","not":[null,null,42]}}
```

and `pretty.json`

```json
{
    "roses": "red",
    "violets": "blue",
    "think of a": "number",
    "was your number": 2
}
```

### Features

- Accept input from stdin, print to stdout, useful for shell pipelines.
- Safe in-place modifcation, files are not modified unless success is guaranteed.
- Straightforward format control: tabs or spaces, or no whitespace at all.
- Fast!  See [the performance comparison section.](#performance-comparison)

### Examples

#### Pretty-printing a JSON blob to a new file

```bash
> cat compact.json | jfmt --output-file not-compact.json
> cat not-compact.json
{
    "1": 2,
    "true": false,
    "null": {
        "very": "much",
        "not": [
            null,
            null,
            42
        ]
    }
}
```

#### Compacting a JSON file

```bash
> jfmt --compact pretty.json
{"roses":"red","violets":"blue","think of a":"number","was your number":2}
```

#### Modifying a JSON file's indentation to 2 spaces

```bash
> jfmt --in-place --spaces 2 pretty.json
> cat pretty.json
{
  "roses": "red",
  "violets": "blue",
  "think of a": "number",
  "was your number": 2
}
```

## Comparison to similar tools

### Why not use `jq`?

Simply put, if you're only looking for formatting, `jfmt` is a much simpler
tool.  While `jq --help` is both well-written and concise, `jq` does more,
and has more to explain.  Additionally `jq` has be able to make JSON
modifications, which requires a full JSON handling engine and query language.

`jfmt` is also about 3-4 times faster than `jq`, mostly because it simply does
less. Try it yourself if you're unsure.

With that said, `jfmt` offers none of the query/modify features that `jq` does,
and never will.  If you need that, I personally recommend using `jq`.

### Why not use `json_pp`?

Ok, be honest, did you know about `json_pp`?  Short version: it's a JSON
format transcoder (take input of format A, produce output of format B) written
in Perl.  It produces other formats, including a Perl-specific format (or
whatever the Dumper format actually is, I don't care to check.)

Comparing `jq` and `json_pp` on the same 25MB input shows `json_pp` to be over
20 times slower than `jq` (which is about 3-4 times slower than jfmt on the
same).  `json_pp` also does not have any options for dealing with files, only
stdin and stdout.

Unless you need the Perl stuff (may god have mercy on your soul), just use
`jfmt` or `jq`.

### Why not use `$MY_FAVORITE_TOOL`?

Because I haven't heard of it yet.  Let me know about it and I'll look into
it, as I have use these tools A LOT.  I want this tool to be really good, or
a better tool to come forward.

### Performance comparison

THESE ARE NOT REAL BENCHMARKS, I KNOW THESE AREN'T LEGITIMATE COMPARISONS.
I'm not trying to formally prove that `jfmt` is faster than other tools, but
for me it is much simpler and consistently faster, so I'm going to use it
until something better comes along.  I swear, if I see even one **"BuT tHoSe
BeNcHmArKs WeReNt PrOpErLy CoNdUcTeD"** issue, I'm going to call you a dumb,
stupid, idiot for not reading this first.

However if you want to run real, actual benchmarks, feel free to run them
and send them to me, or just post them in an issue! I'd be happy to see
how `jfmt` ranks up in the general case, and just as happy to publish any
legit results.

#### Small files

As previously stated, `jfmt` has minimal startup costs, and therefore will
consistently outperform `jq` and `json_pp`, as well as almost any
JSON-evaluating tool, when run against smaller files.  Here are simple
runs against the `compact.json` from the [Examples](#Examples) section.

These were run as `time cat compact.json | $FORMATTER`. Running `time cat
compact.json` showed average times of ~700us, so we can use that to subtract
the running time of cat, leaving us with very little overhead variance, and
a rough estimate of time spent actually running the tools.

| Tool      | Total time    | Time without `cat`    | `jfmt` speedup factor |
| ---       | ---           | ---                   | ---                   |
| `jfmt`    | 837us         | 137us                 | N/A                   |
| `jq`      | 1,750us       | 1,050us               | 7.7x                  |
| `json_pp` | 16,200us      | 15,500us              | 113x                  |

#### Large files

Large file benchmarks are done using [this ~25MB JSON file](bigjson).  This
was found by googling `Large json files`, and is not tuned for performance on
any of these tools.

All of these were run with `time cat large_file.json | $FORMATTER > /dev/null`.
They were piped to `/dev/null` to remove variance introduced by the terminal/shell.
I have also included `json_pp -t null` here, which had a surprising impact, though
still trvial compared to `jfmt`.

`time cat large_file.json > /dev/null` runs at about 16.67ms, so we'll cut that
off of the total time to give us our rough actual time estimate.

For `jq`, you have to specify a filter when piping to `/dev/null` (honestly not
sure why).  We use the passthough filter of `.` to give us a final invocation of:

```bash
time cat large_file.json | jq '.' > /dev/null
```

For `json_pp -t null`, we omit the trailing `> /dev/null`, since that is the
purpose of `-t null`.

| Tool              | Total Time    | Time without `cat`    | `jfmt` speedup factor |
| ---               | ---           | ---                   | ---                   |
| `jfmt`            | 371.88ms      | 355.21ms              | N/A                   |
| `jq '.'`          | 1,250ms       | 1,233.33ms            | 3.5x                  |
| `json_pp`         | 27,430ms      | 27,413.33ms           | 77x                   |
| `json_pp -t null` | 25,310ms      | 25,293.33ms           | 71x                   |

[bigjson]: https://github.com/json-iterator/test-data/blob/master/large-file.json
