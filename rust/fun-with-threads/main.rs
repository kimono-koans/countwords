// This version is an approximate port of the optimized Go program. Its buffer
// handling is slightly simpler: we don't bother with dealing with the last
// newline character. (This may appear to save work, but it only saves work
// once per 64KB buffer, so is likely negligible. It's just simpler IMO.)
//
// There's nothing particularly interesting here other than swapping out std's
// default hashing algorithm for one that isn't cryptographically secure.

use std::{
    error::Error,
    io::{self, BufRead, BufReader, BufWriter, Stdin, Write},
    thread,
};

// std uses a cryptographically secure hashing algorithm by default, which is
// a bit slower. In this particular program, fxhash and fnv seem to perform
// similarly, with fxhash being a touch faster in my ad hoc benchmarks. If
// we wanted to really enforce the "no external crate" rule, we could just
// hand-roll an fnv hash impl ourselves very easily.
//
// N.B. This crate brings in a new hashing function. We still use std's hashmap
// implementation.
//
// Update, RBS 07/26/2022: Since Rust 1.36, hashbrown is the new hashmap impl of the
// stdlib, but this crate includes an additional method, insert_unique_unchecked(),
// which allows us to avoid duplicating hashmap lookups, while avoiding the
// additional alloc of entry().  Moreover, ahash is the hash function of hashbrown,
// which is slightly slower than fxhash when used with the stdlib hashmap, but which
// is slightly faster as used here.
use hashbrown::HashMap;

// attempt to use threading
use crossbeam::channel::{unbounded, Receiver, Sender};

// this in buffer size seems to be slightly faster than 65_536
const IN_BUFFER_SIZE: usize = 131_072;
// this out buffer size seems to be slightly faster than 65_536
const OUT_BUFFER_SIZE: usize = 32_768;
// set hashmap capacity to >= unique words, so we don't allocate again
const HASHMAP_INITIAL_CAPACITY: usize = 32_768;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}

// Update, RBS 07/26/2022: Meat of the changes made are about trying to do something similar to
// the optimized version without doing anything unsafe/unchecked, which feels like readable, relatively
// understandable/simple, idiomatic Rust (nothing too galaxy brained).  This has, surprisingly,
// turned out to be much faster than the optimized version on MacOS/M1 and similar in performance to the
// optimized version on the x86_64/Linux
fn try_main() -> Result<(), Box<dyn Error>> {
    let mut counts: HashMap<Box<str>, usize> = HashMap::with_capacity(HASHMAP_INITIAL_CAPACITY);

    let mut in_buffer = BufReader::with_capacity(IN_BUFFER_SIZE, io::stdin());
    let mut out_buffer = BufWriter::with_capacity(OUT_BUFFER_SIZE, io::stdout());

    // in contrast with the simple/naive version, whole idea is to work on a much larger
    // number of bytes, therefore we should avoid manipulating small buffers, like those
    // created by lines(), as much as we can, and to avoid allocating as much as possible

    let (tx1, rx1) = unbounded();
    let (tx2, rx2) = unbounded();

    thread::spawn(move || {
        let _ = ready_bytes_buffer(&mut in_buffer, tx1);
    });

    thread::spawn(move || {
        let _ = ready_words(rx1, tx2);
    });

    while let Ok(word) = rx2.recv() {
        increment(&mut counts, word);
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
            // so we flush here at the last moment
            out_buffer.flush()?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

fn ready_words(rx: Receiver<Vec<u8>>, tx: Sender<Box<str>>) -> Result<(), Box<dyn Error>> {
    while let Ok(mut bytes_buffer) = rx.recv() {
        let _ = std::str::from_utf8_mut(&mut bytes_buffer)?
            .split_ascii_whitespace()
            .try_for_each(|word| tx.send(Box::from(word)));
    }

    Ok(())
}

fn ready_bytes_buffer(
    in_buffer: &mut BufReader<Stdin>,
    tx: Sender<Vec<u8>>,
) -> Result<(), Box<dyn Error>> {
    loop {
        // first, read lots of bytes into the buffer
        let mut bytes_buffer = in_buffer.fill_buf()?.to_vec();
        in_buffer.consume(bytes_buffer.len());

        // now, keep reading to make sure we haven't stopped in the middle of a word.
        // no need to add the bytes to the total buf_len, as these bytes are auto-"consumed()",
        // and bytes_buffer will be extended from slice to accommodate the new bytes
        in_buffer.read_until(b'\n', &mut bytes_buffer)?;

        // break when there is nothing left to read
        if bytes_buffer.is_empty() {
            break;
        }

        // make_ascii_lowercase on str requires a call to as_bytes(), so use here on
        // directly bytes, but there doesn't seem to be perf advantage
        bytes_buffer.make_ascii_lowercase();

        tx.send(bytes_buffer)?;
    }

    Ok(())
}

fn increment(counts: &mut HashMap<Box<str>, usize>, word: Box<str>) {
    // using 'counts.entry' would be more idiomatic here, but doing so requires
    // allocating a new Vec<u8> because of its API. Instead, we do two hash
    // lookups, but in the exceptionally common case (we see a word we've
    // already seen), we only do one and without any allocs.
    //
    // Update, RBS 07/26/2022: insert_unique_unchecked() allows us to avoid
    // duplicating hashmap lookups, while avoiding the additional alloc of entry.
    // Optimized stores keys as Vec<u8>.  Here, we've already converted to &str,
    // so we Box and save 8 bytes per key compared to storing as a String
    match counts.get_mut(&word) {
        Some(count) => {
            *count += 1;
        }
        None => {
            // safe because we check for the key just above
            counts.insert_unique_unchecked(word, 1);
        }
    }
}
