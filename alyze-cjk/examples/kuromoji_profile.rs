//! Tokenizes Japanese text in a loop for profiling (e.g. `samply record` or macOS `sample`):
//!
//!     CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release -p alyze-cjk --example kuromoji_profile
//!     target/release/examples/kuromoji_profile [seconds] [case file]
//!
//! Defaults to 15 seconds over `testdata/kuromoji/cases/wiki_ja.txt` with the `kuromoji` analyzer and
//! the tokenizer alternating; prints the throughput of each at the end.

use std::time::{Duration, Instant};

use alyze_cjk::kuromoji;

fn main() {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(15);
    let path = args.next().unwrap_or_else(|| {
        format!(
            "{}/testdata/kuromoji/cases/wiki_ja.txt",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    let inputs: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| {
            line.replace("\\n", "\n")
                .replace("\\t", "\t")
                .replace("\\\\", "\\")
        })
        .collect();
    let bytes: usize = inputs.iter().map(String::len).sum();

    let mut tokens = kuromoji::Tokens::new();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut tokenize_time = Duration::ZERO;
    let mut analyze_time = Duration::ZERO;
    let mut rounds = 0usize;
    let mut count = 0usize;
    while Instant::now() < deadline {
        let t = Instant::now();
        for input in &inputs {
            kuromoji::tokenize(input, kuromoji::Options::default(), &mut tokens);
            count += tokens.len();
        }
        tokenize_time += t.elapsed();
        let t = Instant::now();
        for input in &inputs {
            kuromoji::analyze(input, kuromoji::AnalyzerOptions::default(), &mut tokens);
            count += tokens.len();
        }
        analyze_time += t.elapsed();
        rounds += 1;
    }
    let mib = |d: Duration| (bytes * rounds) as f64 / 1048576.0 / d.as_secs_f64();
    println!(
        "{rounds} rounds over {bytes} bytes: tokenize {:.1} MiB/s, analyze {:.1} MiB/s ({count} tokens)",
        mib(tokenize_time),
        mib(analyze_time)
    );
}
