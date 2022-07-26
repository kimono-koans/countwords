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
use hashbrown::HashMap;

const BUFFER_SIZE: usize = 131_072;
// set hashmap capacity to >= unique words, so we don't allocate again
const HASHMAP_INITIAL_CAPACITY: usize = 65_536;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), Box<dyn Error>> {
    let mut counts: HashMap<Box<str>, usize> = HashMap::with_capacity(HASHMAP_INITIAL_CAPACITY);

    let mut in_buffer = BufReader::with_capacity(BUFFER_SIZE, io::stdin());
    let mut out_buffer = BufWriter::with_capacity(BUFFER_SIZE, io::stdout());

    loop {
        // read into the buffer
        let mut bytes_buffer = in_buffer.fill_buf()?.to_vec();
        // need to know how much we've read in to consume() later
        let buf_len = bytes_buffer.len();
        // finally consume()
        in_buffer.consume(buf_len);

        // these are auto-"consumed()" no need to add to the total buf_len
        // and the bytes_buffer will extend_from_slice to accommodate
        let _num_additional_bytes = in_buffer.read_until(b'\n', &mut bytes_buffer)?;

        // break when there is nothing left to read
        if bytes_buffer.is_empty() {
            break;
        }

        // make_ascii_lowercase on str requires a call to as_bytes
        // so we use here on bytes, but there doesn't seem to be perf advantage
        bytes_buffer.make_ascii_lowercase();

        // don't need to worry about lines, if we know the buffer terminates in a newline
        // and we are splitting on whitespace which includes newlines
        //
        // avoid allocating by using make_ascii_lowercase() and from_utf8_mut(), which converts in place
        std::str::from_utf8_mut(&mut bytes_buffer)?
            .split_ascii_whitespace()
            .for_each(|word| increment(&mut counts, word));
    }

    let mut ordered: Vec<_> = counts.into_iter().collect();
    ordered.sort_unstable_by_key(|&(_, count)| count);

    let ret = ordered
        .into_iter()
        .rev()
        .try_for_each(|(word, count)| writeln!(out_buffer, "{} {}", word, count));

    match ret {
        Ok(_) => {
            // docs say its critical to do a flush before drop
            out_buffer.flush()?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn increment(counts: &mut HashMap<Box<str>, usize>, word: &str) {
    // using 'counts.entry' would be more idiomatic here, but doing so requires
    // allocating a new Vec<u8> because of its API. Instead, we do two hash
    // lookups, but in the exceptionally common case (we see a word we've
    // already seen), we only do one and without any allocs.
    match counts.get_mut(word) {
        Some(count) => {
            *count += 1;
        }
        None => {
            // safe because we check for the key just above
            counts.insert_unique_unchecked(Box::from(word), 1);
        }
    }
}
