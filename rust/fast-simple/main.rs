// This version is an approximate port of the optimized Go program. Its buffer
// handling is slightly simpler: we don't bother with dealing with the last
// newline character. (This may appear to save work, but it only saves work
// once per 64KB buffer, so is likely negligible. It's just simpler IMO.)
//
// There's nothing particularly interesting here other than swapping out std's
// default hashing algorithm for one that isn't cryptographically secure.

use std::{
    error::Error,
    io::{self, BufRead, BufReader, BufWriter, Write},
};

// std uses a cryptographically secure hashing algorithm by default, which is
// a bit slower. In this particular program, fxhash and fnv seem to perform
// similarly, with fxhash being a touch faster in my ad hoc benchmarks. If
// we wanted to really enforce the "no external crate" rule, we could just
// hand-roll an fnv hash impl ourselves very easily.
//
// N.B. This crate brings in a new hashing function. We still use std's hashmap
// implementation.
use fxhash::FxHashMap as HashMap;

const BUFFER_SIZE: usize = 65_536;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let mut counts: HashMap<String, u64> = HashMap::default();

    let mut in_buffer = BufReader::with_capacity(BUFFER_SIZE, io::stdin());
    let mut out_buffer = BufWriter::with_capacity(BUFFER_SIZE, io::stdout());

    loop {
        // read into the buffer
        let mut string_buffer = std::str::from_utf8(in_buffer.fill_buf()?)?.to_owned();

        // make sure we catch everything up to a new line
        in_buffer.read_line(&mut string_buffer)?;

        // break when there is nothing left to read
        if string_buffer.is_empty() {
            break;
        }

        // need to know how much we've read in total to consume() later
        let buf_len = string_buffer.len();

        // don't need to worry about lines, if we know
        // the buffer terminates in a new line
        string_buffer
            .to_ascii_lowercase()
            .split_ascii_whitespace()
            .for_each(|word| increment(&mut counts, word));

        // finally consume() and get ready to fill_buf() again
        in_buffer.consume(buf_len);
    }

    let mut ordered: Vec<_> = counts.into_iter().collect();
    ordered.sort_unstable_by_key(|&(_, count)| count);

    ordered.into_iter().rev().try_for_each(|(word, count)| {
        writeln!(out_buffer, "{} {}", word, count).map_err(|err| err.into())
    })
}

fn increment(counts: &mut HashMap<String, u64>, word: &str) {
    // using 'counts.entry' would be more idiomatic here, but doing so requires
    // allocating a new Vec<u8> because of its API. Instead, we do two hash
    // lookups, but in the exceptionally common case (we see a word we've
    // already seen), we only do one and without any allocs.
    if let Some(count) = counts.get_mut(word) {
        *count += 1;
        return;
    }
    counts.insert(word.to_owned(), 1);
}
