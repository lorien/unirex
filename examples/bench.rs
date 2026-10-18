//! Engine-only Telegram benchmark.
//!
//! The harness is self-contained: it resolves the corpus from `--corpus`, an
//! environment variable, a cached copy under `var/`, or the public release URL
//! (SHA-256 verified). It never depends on another checkout.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Barrier;
use std::time::{Duration, Instant};

use exp_rurex::{Regex, Scratch, Ucs};
use flate2::read::MultiGzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DATA_URL: &str =
    "https://github.com/lorien/rurex/releases/download/bench-data-telegram-v1/tg-1m.json.gz";
const DATA_SHA256: &str = "eb91db735fcf1a9e6096a59e959471cb9a3c4f41438f864e800cdb66f994023b";
const DATA_CACHE: &str = "var/bench-data/tg-1m.json.gz";
const DEFAULT_PATTERN: &str = r"@[a-zA-Z][a-zA-Z0-9_]{4,31}";
const FULL_POSTS: usize = 1_000_000;
const FULL_MENTIONS: usize = 334_136;
const FULL_POSTS_WITH: usize = 186_226;
const DOWNLOAD_LIMIT: u64 = 200 * 1024 * 1024;
const SELF_TEST_JSONL: &str = concat!(
    "{\"text\":\"hello @alice123 and @bob_42\"}\n",
    "{\"text\":\"@abc\"}\n",
    "{\"text\":\"@abcd\"}\n",
    "{\"text\":\"@abcde\"}\n",
    "{\"text\":\"@1abcde\"}\n",
    "{\"text\":\"@@abcde\"}\n",
    "{\"text\":\"مرحبا @user_name\"}\n",
    "{\"text\":\"😀 @emoji_user\"}\n",
    "{\"text\":\"@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}\n",
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Ucs,
    Bytes,
    Str,
}

impl Mode {
    fn parse(value: &str) -> Option<Mode> {
        match value {
            "ucs" => Some(Mode::Ucs),
            "bytes" => Some(Mode::Bytes),
            "str" => Some(Mode::Str),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Mode::Ucs => "ucs",
            Mode::Bytes => "bytes",
            Mode::Str => "str",
        }
    }
}

struct Options {
    corpus: Option<PathBuf>,
    mode: Mode,
    limit: Option<usize>,
    repeat: usize,
    warmup: usize,
    warmup_seconds: f64,
    min_efficiency: f64,
    retries: usize,
    threads: usize,
    cpu_list: Vec<usize>,
    max_load: Option<f64>,
    pattern: String,
    json: Option<PathBuf>,
    expect_mentions: Option<usize>,
    expect_posts: Option<usize>,
    no_download: bool,
    self_test: bool,
    help: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            corpus: None,
            mode: Mode::Ucs,
            limit: None,
            repeat: 3,
            warmup: 1,
            warmup_seconds: 1.0,
            min_efficiency: 0.90,
            retries: 2,
            threads: 1,
            cpu_list: Vec::new(),
            max_load: None,
            pattern: DEFAULT_PATTERN.to_string(),
            json: None,
            expect_mentions: None,
            expect_posts: None,
            no_download: false,
            self_test: false,
            help: false,
        }
    }
}

impl Options {
    fn effective_threads(&self) -> usize {
        if self.cpu_list.is_empty() {
            self.threads
        } else {
            self.threads.min(self.cpu_list.len()).max(1)
        }
    }

    fn sampled_cpus(&self) -> Vec<usize> {
        if self.cpu_list.is_empty() {
            vec![0]
        } else {
            self.cpu_list.clone()
        }
    }
}

struct EnvSample {
    threads: usize,
    load_before: f64,
    load_after: f64,
    freq_before_mhz: Vec<u64>,
    freq_after_mhz: Vec<u64>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("bench: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let opts = parse_args()?;
    if opts.help {
        print_usage();
        return Ok(());
    }
    if opts.self_test {
        return self_test();
    }
    if cfg!(debug_assertions) {
        eprintln!("warning: debug build; timings are not meaningful. Use --release.");
    }
    if let Some(max_load) = opts.max_load {
        let load = read_load();
        if load > max_load {
            return Err(format!(
                "system load {load:.2} exceeds --max-load {max_load:.2}; retry when idle"
            ));
        }
    }

    let (corpus_path, corpus_sha256) = resolve_corpus(&opts)?;
    let report = match opts.mode {
        Mode::Ucs => {
            let corpus = load_ucs(&corpus_path, opts.limit)?;
            validate_expected(
                corpus.expected_mentions,
                corpus.expected_posts_with,
                opts.limit,
                corpus.posts.len(),
            )?;
            let re = Regex::new(&opts.pattern).map_err(|e| format!("pattern: {e}"))?;
            let threads = opts.effective_threads().min(corpus.posts.len().max(1));
            let mut scratches: Vec<_> = (0..threads).map(|_| re.scratch()).collect();
            let load_before = read_load();
            warmup(
                || {
                    scan_ucs(&re, &corpus, &mut scratches, &opts.cpu_list);
                },
                &opts,
            );
            let freq_before_mhz = sample_freqs(&opts.sampled_cpus());
            let runs = measure(
                || scan_ucs(&re, &corpus, &mut scratches, &opts.cpu_list),
                &opts,
            )?;
            let freq_after_mhz = sample_freqs(&opts.sampled_cpus());
            let env = EnvSample {
                threads,
                load_before,
                load_after: read_load(),
                freq_before_mhz,
                freq_after_mhz,
            };
            validate_counts(
                &opts,
                runs[0].count,
                runs[0].posts_with,
                corpus.expected_mentions,
                corpus.expected_posts_with,
            )?;
            build_report(
                &opts,
                &corpus_path,
                corpus_sha256.clone(),
                Mode::Ucs,
                CorpusShape {
                    posts: corpus.posts.len(),
                    utf8_bytes: corpus.utf8_bytes,
                    units: corpus.units,
                    kind_posts: corpus.kind_posts,
                    kind_units: corpus.kind_units,
                    expected_mentions: corpus.expected_mentions,
                    expected_posts_with: corpus.expected_posts_with,
                },
                &runs,
                &env,
            )
        }
        Mode::Bytes => {
            let corpus = load_bytes(&corpus_path, opts.limit)?;
            validate_expected(
                corpus.expected_mentions,
                corpus.expected_posts_with,
                opts.limit,
                corpus.posts.len(),
            )?;
            let re =
                regex::bytes::Regex::new(&opts.pattern).map_err(|e| format!("pattern: {e}"))?;
            let threads = opts.effective_threads().min(corpus.posts.len().max(1));
            let load_before = read_load();
            warmup(
                || {
                    scan_bytes(&re, &corpus, threads, &opts.cpu_list);
                },
                &opts,
            );
            let freq_before_mhz = sample_freqs(&opts.sampled_cpus());
            let runs = measure(|| scan_bytes(&re, &corpus, threads, &opts.cpu_list), &opts)?;
            let freq_after_mhz = sample_freqs(&opts.sampled_cpus());
            let env = EnvSample {
                threads,
                load_before,
                load_after: read_load(),
                freq_before_mhz,
                freq_after_mhz,
            };
            validate_counts(
                &opts,
                runs[0].count,
                runs[0].posts_with,
                corpus.expected_mentions,
                corpus.expected_posts_with,
            )?;
            build_report(
                &opts,
                &corpus_path,
                corpus_sha256.clone(),
                Mode::Bytes,
                CorpusShape {
                    posts: corpus.posts.len(),
                    utf8_bytes: corpus.utf8_bytes,
                    units: corpus.utf8_bytes,
                    kind_posts: [0; 3],
                    kind_units: [0; 3],
                    expected_mentions: corpus.expected_mentions,
                    expected_posts_with: corpus.expected_posts_with,
                },
                &runs,
                &env,
            )
        }
        Mode::Str => {
            let corpus = load_bytes(&corpus_path, opts.limit)?;
            validate_expected(
                corpus.expected_mentions,
                corpus.expected_posts_with,
                opts.limit,
                corpus.posts.len(),
            )?;
            let re = regex::Regex::new(&opts.pattern).map_err(|e| format!("pattern: {e}"))?;
            let threads = opts.effective_threads().min(corpus.posts.len().max(1));
            let load_before = read_load();
            warmup(
                || {
                    scan_str(&re, &corpus, threads, &opts.cpu_list);
                },
                &opts,
            );
            let freq_before_mhz = sample_freqs(&opts.sampled_cpus());
            let runs = measure(|| scan_str(&re, &corpus, threads, &opts.cpu_list), &opts)?;
            let freq_after_mhz = sample_freqs(&opts.sampled_cpus());
            let env = EnvSample {
                threads,
                load_before,
                load_after: read_load(),
                freq_before_mhz,
                freq_after_mhz,
            };
            validate_counts(
                &opts,
                runs[0].count,
                runs[0].posts_with,
                corpus.expected_mentions,
                corpus.expected_posts_with,
            )?;
            build_report(
                &opts,
                &corpus_path,
                corpus_sha256.clone(),
                Mode::Str,
                CorpusShape {
                    posts: corpus.posts.len(),
                    utf8_bytes: corpus.utf8_bytes,
                    units: corpus.utf8_bytes,
                    kind_posts: [0; 3],
                    kind_units: [0; 3],
                    expected_mentions: corpus.expected_mentions,
                    expected_posts_with: corpus.expected_posts_with,
                },
                &runs,
                &env,
            )
        }
    };

    let human = format!(
        "mode={} posts={} {}={} expected={} first={:.6}s median={:.6}s min={:.6}s {:.1}ns/post {:.1}MB/s",
        report.mode,
        report.posts,
        report.count_kind,
        report.count,
        report.expected_mentions,
        report.first_s,
        report.median_s,
        report.min_s,
        report.ns_per_post,
        report.mb_per_s,
    );
    println!(
        "cpu={:.6}s efficiency={:.2} load={:.2}->{:.2} freq={:?}->{:?} cores={:?}",
        report.first_cpu_s,
        report.cpu_efficiency,
        report.load_before,
        report.load_after,
        report.freq_before_mhz,
        report.freq_after_mhz,
        report.worker_cores,
    );
    println!("{human}");
    if let Some(path) = &opts.json {
        let text =
            serde_json::to_string_pretty(&report).map_err(|e| format!("serialize report: {e}"))?;
        fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn print_usage() {
    println!(
        "usage: cargo run --release --example bench -- [options]\n\
         \n\
         options:\n\
           --corpus PATH        corpus .jsonl or .json.gz (overrides cache/download)\n\
           --mode MODE          ucs (default), bytes, str\n\
           --limit N            load only the first N posts\n\
           --repeat N           timed runs (default 3)\n\
           --warmup N           minimum untimed warmup passes (default 1)\n\
           --warmup-seconds F   minimum warmup duration (default 1.0)\n\
           --min-efficiency F   discard runs below this CPU/wall ratio (default 0.90)\n\
           --retries N          extra attempts after a discarded run (default 2)\n\
           --threads N          worker threads (default 1)\n\
           --cpu-list LIST      pin workers, e.g. 2 or 0,2,4,6 or 0-3\n\
           --max-load F         abort if 1-minute load is above F\n\
           --pattern PATTERN    pattern (default Telegram @username pattern)\n\
           --json PATH          write a JSON report\n\
           --expect-mentions N  extra correctness check\n\
           --expect-posts N     extra correctness check\n\
           --no-download        fail if the cache is missing instead of downloading\n\
           --self-test          run the embedded offline sanity test\n\
           --help               show this help\n\
         \n\
         environment:\n\
           RUREX_TG_DATA        corpus path, equivalent to --corpus\n\
         \n\
         corpus cache: {DATA_CACHE}\n\
         public URL:   {DATA_URL}\n\
         sha256:       {DATA_SHA256}"
    );
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--corpus" => opts.corpus = Some(PathBuf::from(next_value(&mut args, "--corpus")?)),
            "--mode" => {
                let value = next_value(&mut args, "--mode")?;
                opts.mode = Mode::parse(&value)
                    .ok_or_else(|| format!("unknown mode {value:?}; use ucs, bytes, or str"))?;
            }
            "--limit" => {
                let value: usize = next_value(&mut args, "--limit")?
                    .parse()
                    .map_err(|_| "--limit expects a positive integer".to_string())?;
                if value == 0 {
                    return Err("--limit must be greater than zero".to_string());
                }
                opts.limit = Some(value);
            }
            "--repeat" => {
                let value: usize = next_value(&mut args, "--repeat")?
                    .parse()
                    .map_err(|_| "--repeat expects a positive integer".to_string())?;
                if value == 0 {
                    return Err("--repeat must be greater than zero".to_string());
                }
                opts.repeat = value;
            }
            "--warmup" => {
                opts.warmup = next_value(&mut args, "--warmup")?
                    .parse()
                    .map_err(|_| "--warmup expects an integer".to_string())?;
            }
            "--warmup-seconds" => {
                let value: f64 = next_value(&mut args, "--warmup-seconds")?
                    .parse()
                    .map_err(|_| "--warmup-seconds expects a number".to_string())?;
                if value < 0.0 {
                    return Err("--warmup-seconds must be non-negative".to_string());
                }
                opts.warmup_seconds = value;
            }
            "--threads" => {
                let value: usize = next_value(&mut args, "--threads")?
                    .parse()
                    .map_err(|_| "--threads expects a positive integer".to_string())?;
                if value == 0 {
                    return Err("--threads must be greater than zero".to_string());
                }
                opts.threads = value;
            }
            "--cpu-list" => {
                opts.cpu_list = parse_cpu_list(&next_value(&mut args, "--cpu-list")?)?;
            }
            "--min-efficiency" => {
                let value: f64 = next_value(&mut args, "--min-efficiency")?
                    .parse()
                    .map_err(|_| "--min-efficiency expects a number".to_string())?;
                if !(0.0..=1.0).contains(&value) {
                    return Err("--min-efficiency must be between 0 and 1".to_string());
                }
                opts.min_efficiency = value;
            }
            "--retries" => {
                opts.retries = next_value(&mut args, "--retries")?
                    .parse()
                    .map_err(|_| "--retries expects an integer".to_string())?;
            }
            "--max-load" => {
                opts.max_load = Some(
                    next_value(&mut args, "--max-load")?
                        .parse()
                        .map_err(|_| "--max-load expects a number".to_string())?,
                );
            }
            "--pattern" => opts.pattern = next_value(&mut args, "--pattern")?,
            "--json" => opts.json = Some(PathBuf::from(next_value(&mut args, "--json")?)),
            "--expect-mentions" => {
                opts.expect_mentions = Some(
                    next_value(&mut args, "--expect-mentions")?
                        .parse()
                        .map_err(|_| "--expect-mentions expects an integer".to_string())?,
                );
            }
            "--expect-posts" => {
                opts.expect_posts = Some(
                    next_value(&mut args, "--expect-posts")?
                        .parse()
                        .map_err(|_| "--expect-posts expects an integer".to_string())?,
                );
            }
            "--no-download" => opts.no_download = true,
            "--self-test" => opts.self_test = true,
            "--help" | "-h" => opts.help = true,
            other => return Err(format!("unknown argument {other:?}; use --help")),
        }
    }
    Ok(opts)
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn resolve_corpus(opts: &Options) -> Result<(PathBuf, Option<String>), String> {
    if let Some(path) = &opts.corpus {
        if !path.exists() {
            return Err(format!("corpus not found: {}", path.display()));
        }
        return Ok((path.clone(), Some(sha256_file(path)?)));
    }
    if let Some(value) = std::env::var_os("RUREX_TG_DATA") {
        let path = PathBuf::from(value);
        if !path.exists() {
            return Err(format!(
                "RUREX_TG_DATA points to a missing file: {}",
                path.display()
            ));
        }
        let sha = sha256_file(&path)?;
        return Ok((path, Some(sha)));
    }

    let cache = PathBuf::from(DATA_CACHE);
    if cache.exists() {
        let sha = sha256_file(&cache)?;
        if sha != DATA_SHA256 {
            return Err(format!(
                "cached corpus checksum mismatch: {sha} != {DATA_SHA256}; remove {} and retry",
                cache.display()
            ));
        }
        return Ok((cache, Some(sha)));
    }
    if opts.no_download {
        return Err(format!(
            "corpus missing at {}; download {DATA_URL} and verify SHA-256 {DATA_SHA256}, or pass --corpus",
            cache.display()
        ));
    }
    download_corpus(&cache)?;
    let sha = sha256_file(&cache)?;
    Ok((cache, Some(sha)))
}

fn download_corpus(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("part");
    println!("downloading corpus from {DATA_URL}");
    let mut response = ureq::get(DATA_URL)
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(DOWNLOAD_LIMIT)
        .reader();
    let file = File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1 << 20];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("download read failed: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        writer
            .write_all(&buffer[..read])
            .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    }
    writer
        .flush()
        .map_err(|e| format!("flush {}: {e}", tmp.display()))?;
    let digest = hasher.finalize();
    let got = hex(digest.as_slice());
    if got != DATA_SHA256 {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "downloaded corpus checksum mismatch: {got} != {DATA_SHA256}"
        ));
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))?;
    println!("cached corpus at {}", path.display());
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1 << 20];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(hex(digest.as_slice()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackedKind {
    Ucs1,
    Ucs2,
    Ucs4,
}

impl PackedKind {
    fn index(self) -> usize {
        match self {
            PackedKind::Ucs1 => 0,
            PackedKind::Ucs2 => 1,
            PackedKind::Ucs4 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PostRef {
    kind: PackedKind,
    start: u32,
    len: u32,
}

#[derive(Default)]
struct UcsCorpus {
    ucs1: Vec<u8>,
    ucs2: Vec<u16>,
    ucs4: Vec<u32>,
    posts: Vec<PostRef>,
    utf8_bytes: usize,
    units: usize,
    kind_posts: [usize; 3],
    kind_units: [usize; 3],
    expected_mentions: usize,
    expected_posts_with: usize,
}

impl UcsCorpus {
    fn push(&mut self, text: &str) -> Result<(), String> {
        let max = text.chars().map(|c| c as u32).max().unwrap_or(0);
        let kind = if max <= 0xFF {
            PackedKind::Ucs1
        } else if max <= 0xFFFF {
            PackedKind::Ucs2
        } else {
            PackedKind::Ucs4
        };
        let len = text.chars().count();
        let start = match kind {
            PackedKind::Ucs1 => {
                let start = self.ucs1.len();
                self.ucs1.extend(text.chars().map(|c| c as u8));
                start
            }
            PackedKind::Ucs2 => {
                let start = self.ucs2.len();
                self.ucs2.extend(text.chars().map(|c| c as u16));
                start
            }
            PackedKind::Ucs4 => {
                let start = self.ucs4.len();
                self.ucs4.extend(text.chars().map(|c| c as u32));
                start
            }
        };
        let start = u32::try_from(start).map_err(|_| "corpus buffer exceeds u32 offsets")?;
        let len = u32::try_from(len).map_err(|_| "post length exceeds u32")?;
        self.posts.push(PostRef { kind, start, len });
        self.kind_posts[kind.index()] += 1;
        self.kind_units[kind.index()] += len as usize;
        self.units += len as usize;
        self.utf8_bytes += text.len();
        let (mentions, with) = naive_count(text);
        self.expected_mentions += mentions;
        if with {
            self.expected_posts_with += 1;
        }
        Ok(())
    }

    fn ucs(&self, post: PostRef) -> Ucs<'_> {
        let start = post.start as usize;
        let end = start + post.len as usize;
        match post.kind {
            PackedKind::Ucs1 => Ucs::Ucs1(&self.ucs1[start..end]),
            PackedKind::Ucs2 => Ucs::Ucs2(&self.ucs2[start..end]),
            PackedKind::Ucs4 => Ucs::Ucs4(&self.ucs4[start..end]),
        }
    }
}

#[derive(Default)]
struct BytesCorpus {
    data: String,
    posts: Vec<(u32, u32)>,
    utf8_bytes: usize,
    expected_mentions: usize,
    expected_posts_with: usize,
}

impl BytesCorpus {
    fn push(&mut self, text: &str) -> Result<(), String> {
        let start =
            u32::try_from(self.data.len()).map_err(|_| "corpus buffer exceeds u32 offsets")?;
        self.data.push_str(text);
        let end =
            u32::try_from(self.data.len()).map_err(|_| "corpus buffer exceeds u32 offsets")?;
        self.posts.push((start, end));
        self.utf8_bytes += text.len();
        let (mentions, with) = naive_count(text);
        self.expected_mentions += mentions;
        if with {
            self.expected_posts_with += 1;
        }
        Ok(())
    }
}

fn naive_count(text: &str) -> (usize, bool) {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut count = 0;
    while index < bytes.len() {
        if bytes[index] == b'@' && index + 1 < bytes.len() && bytes[index + 1].is_ascii_alphabetic()
        {
            let mut cursor = index + 2;
            let mut extra = 0;
            while cursor < bytes.len() && extra < 31 && is_word_byte(bytes[cursor]) {
                cursor += 1;
                extra += 1;
            }
            if extra >= 4 {
                count += 1;
            }
            index = cursor;
        } else {
            index += 1;
        }
    }
    (count, count > 0)
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[derive(Deserialize)]
struct JsonPost {
    #[serde(default)]
    text: Option<String>,
}

fn load_ucs(path: &Path, limit: Option<usize>) -> Result<UcsCorpus, String> {
    let reader = open_reader(path)?;
    load_ucs_from_reader(reader, limit)
}

fn load_bytes(path: &Path, limit: Option<usize>) -> Result<BytesCorpus, String> {
    let reader = open_reader(path)?;
    load_bytes_from_reader(reader, limit)
}

fn open_reader(path: &Path) -> Result<Box<dyn BufRead>, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let is_gzip = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    if is_gzip {
        Ok(Box::new(BufReader::new(MultiGzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

fn load_ucs_from_reader(reader: impl BufRead, limit: Option<usize>) -> Result<UcsCorpus, String> {
    let mut corpus = UcsCorpus::default();
    for_each_text(reader, limit, |text| corpus.push(text))?;
    Ok(corpus)
}

fn load_bytes_from_reader(
    reader: impl BufRead,
    limit: Option<usize>,
) -> Result<BytesCorpus, String> {
    let mut corpus = BytesCorpus::default();
    for_each_text(reader, limit, |text| corpus.push(text))?;
    Ok(corpus)
}

fn for_each_text(
    mut reader: impl BufRead,
    limit: Option<usize>,
    mut visit: impl FnMut(&str) -> Result<(), String>,
) -> Result<usize, String> {
    let mut line = String::new();
    let mut loaded = 0;
    let mut line_number = 0;
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("read corpus: {e}"))?;
        if read == 0 {
            break;
        }
        line_number += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let post: JsonPost = serde_json::from_str(trimmed)
            .map_err(|e| format!("JSON parse at line {line_number}: {e}"))?;
        if let Some(text) = post.text {
            visit(&text)?;
            loaded += 1;
            if let Some(limit) = limit {
                if loaded >= limit {
                    break;
                }
            }
        }
    }
    Ok(loaded)
}

fn validate_expected(
    expected_mentions: usize,
    expected_posts_with: usize,
    limit: Option<usize>,
    posts: usize,
) -> Result<(), String> {
    if limit.is_none() && posts == FULL_POSTS {
        if expected_mentions != FULL_MENTIONS {
            return Err(format!(
                "naive mention count {expected_mentions} != expected {FULL_MENTIONS}"
            ));
        }
        if expected_posts_with != FULL_POSTS_WITH {
            return Err(format!(
                "naive posts-with-mentions {expected_posts_with} != expected {FULL_POSTS_WITH}"
            ));
        }
    }
    Ok(())
}

fn validate_counts(
    opts: &Options,
    count: usize,
    posts_with: usize,
    expected_mentions: usize,
    expected_posts_with: usize,
) -> Result<(), String> {
    if count != expected_mentions {
        return Err(format!(
            "mention count {count} != independent naive count {expected_mentions}"
        ));
    }
    if posts_with != expected_posts_with {
        return Err(format!(
            "posts with matches {posts_with} != independent naive count {expected_posts_with}"
        ));
    }
    if let Some(expected) = opts.expect_mentions {
        if count != expected {
            return Err(format!(
                "mention count {count} != --expect-mentions {expected}"
            ));
        }
    }
    if let Some(expected) = opts.expect_posts {
        if posts_with != expected {
            return Err(format!(
                "posts with matches {posts_with} != --expect-posts {expected}"
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Trial {
    count: usize,
    posts_with: usize,
    elapsed: Duration,
    cpu: Duration,
    cores: Vec<i32>,
    efficiencies: Vec<f64>,
}

fn combine(trials: Vec<Trial>) -> Trial {
    Trial {
        count: trials.iter().map(|t| t.count).sum(),
        posts_with: trials.iter().map(|t| t.posts_with).sum(),
        elapsed: trials.iter().map(|t| t.elapsed).max().unwrap_or_default(),
        cpu: trials.iter().map(|t| t.cpu).sum(),
        cores: trials
            .iter()
            .flat_map(|t| t.cores.iter().copied())
            .collect(),
        efficiencies: trials
            .iter()
            .flat_map(|t| t.efficiencies.iter().copied())
            .collect(),
    }
}

fn measure<F: FnMut() -> Trial>(mut scan: F, opts: &Options) -> Result<Vec<Trial>, String> {
    let mut runs = Vec::new();
    let attempts_limit = opts.repeat.saturating_add(opts.retries);
    let mut attempts = 0usize;
    while runs.len() < opts.repeat && attempts < attempts_limit {
        attempts += 1;
        let trial = scan();
        let efficiency = trial.efficiencies.iter().copied().fold(1.0_f64, f64::min);
        if efficiency >= opts.min_efficiency {
            runs.push(trial);
        } else {
            eprintln!(
                "discarding run: CPU efficiency {efficiency:.2} below {:.2}; system load?",
                opts.min_efficiency
            );
        }
    }
    if runs.is_empty() {
        return Err(format!(
            "no clean runs: per-worker CPU efficiency stayed below {:.2}; retry when idle",
            opts.min_efficiency
        ));
    }
    if runs.len() < opts.repeat {
        eprintln!(
            "warning: only {} clean runs out of requested {}",
            runs.len(),
            opts.repeat
        );
    }
    Ok(runs)
}

fn warmup<F: FnMut()>(mut scan: F, opts: &Options) {
    let deadline = Instant::now() + Duration::from_secs_f64(opts.warmup_seconds);
    let mut passes = 0usize;
    while passes < opts.warmup || Instant::now() < deadline {
        scan();
        passes += 1;
        if passes > 100_000 {
            break;
        }
    }
}

fn chunk_bounds(posts: usize, threads: usize) -> Vec<(usize, usize)> {
    let threads = threads.min(posts.max(1)).max(1);
    let chunk = posts.div_ceil(threads);
    (0..threads)
        .map(|index| {
            let start = (index * chunk).min(posts);
            let end = ((index + 1) * chunk).min(posts);
            (start, end)
        })
        .collect()
}

fn scan_ucs(re: &Regex, corpus: &UcsCorpus, scratches: &mut [Scratch], cpus: &[usize]) -> Trial {
    let bounds = chunk_bounds(corpus.posts.len(), scratches.len());
    let barrier = Barrier::new(bounds.len());
    let trials = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(bounds.len());
        let mut scratch_iter = scratches.iter_mut();
        for (index, (start, end)) in bounds.into_iter().enumerate() {
            let scratch = scratch_iter.next().expect("one scratch per worker");
            let cpu = cpus.get(index).copied();
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                if let Some(cpu) = cpu {
                    pin_thread(cpu);
                }
                let core = current_cpu();
                barrier.wait();
                let cpu_start = thread_cpu_time();
                let timer = Instant::now();
                let mut count = 0;
                let mut posts_with = 0;
                for post in &corpus.posts[start..end] {
                    let mut matches = 0;
                    for m in re.find_iter_with(scratch, corpus.ucs(*post)) {
                        matches += 1;
                        std::hint::black_box(m.end());
                    }
                    count += matches;
                    if matches > 0 {
                        posts_with += 1;
                    }
                }
                std::hint::black_box(count);
                let elapsed = timer.elapsed();
                let cpu = thread_cpu_time().saturating_sub(cpu_start);
                let efficiency = if elapsed.as_secs_f64() > 0.0 {
                    cpu.as_secs_f64() / elapsed.as_secs_f64()
                } else {
                    0.0
                };
                Trial {
                    count,
                    posts_with,
                    elapsed,
                    cpu,
                    cores: vec![core],
                    efficiencies: vec![efficiency],
                }
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("worker panicked"))
            .collect::<Vec<_>>()
    });
    combine(trials)
}

fn scan_bytes(
    re: &regex::bytes::Regex,
    corpus: &BytesCorpus,
    threads: usize,
    cpus: &[usize],
) -> Trial {
    let bounds = chunk_bounds(corpus.posts.len(), threads);
    let barrier = Barrier::new(bounds.len());
    let data = corpus.data.as_bytes();
    let trials = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(bounds.len());
        for (index, (start, end)) in bounds.into_iter().enumerate() {
            let cpu = cpus.get(index).copied();
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                if let Some(cpu) = cpu {
                    pin_thread(cpu);
                }
                let core = current_cpu();
                barrier.wait();
                let cpu_start = thread_cpu_time();
                let timer = Instant::now();
                let mut count = 0;
                let mut posts_with = 0;
                for &(post_start, post_end) in &corpus.posts[start..end] {
                    let slice = &data[post_start as usize..post_end as usize];
                    let mut matches = 0;
                    for m in re.find_iter(slice) {
                        matches += 1;
                        std::hint::black_box(m.end());
                    }
                    count += matches;
                    if matches > 0 {
                        posts_with += 1;
                    }
                }
                std::hint::black_box(count);
                let elapsed = timer.elapsed();
                let cpu = thread_cpu_time().saturating_sub(cpu_start);
                let efficiency = if elapsed.as_secs_f64() > 0.0 {
                    cpu.as_secs_f64() / elapsed.as_secs_f64()
                } else {
                    0.0
                };
                Trial {
                    count,
                    posts_with,
                    elapsed,
                    cpu,
                    cores: vec![core],
                    efficiencies: vec![efficiency],
                }
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("worker panicked"))
            .collect::<Vec<_>>()
    });
    combine(trials)
}

fn scan_str(re: &regex::Regex, corpus: &BytesCorpus, threads: usize, cpus: &[usize]) -> Trial {
    let bounds = chunk_bounds(corpus.posts.len(), threads);
    let barrier = Barrier::new(bounds.len());
    let data = corpus.data.as_str();
    let trials = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(bounds.len());
        for (index, (start, end)) in bounds.into_iter().enumerate() {
            let cpu = cpus.get(index).copied();
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                if let Some(cpu) = cpu {
                    pin_thread(cpu);
                }
                let core = current_cpu();
                barrier.wait();
                let cpu_start = thread_cpu_time();
                let timer = Instant::now();
                let mut count = 0;
                let mut posts_with = 0;
                for &(post_start, post_end) in &corpus.posts[start..end] {
                    let slice = &data[post_start as usize..post_end as usize];
                    let mut matches = 0;
                    for m in re.find_iter(slice) {
                        matches += 1;
                        std::hint::black_box(m.end());
                    }
                    count += matches;
                    if matches > 0 {
                        posts_with += 1;
                    }
                }
                std::hint::black_box(count);
                let elapsed = timer.elapsed();
                let cpu = thread_cpu_time().saturating_sub(cpu_start);
                let efficiency = if elapsed.as_secs_f64() > 0.0 {
                    cpu.as_secs_f64() / elapsed.as_secs_f64()
                } else {
                    0.0
                };
                Trial {
                    count,
                    posts_with,
                    elapsed,
                    cpu,
                    cores: vec![core],
                    efficiencies: vec![efficiency],
                }
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("worker panicked"))
            .collect::<Vec<_>>()
    });
    combine(trials)
}

fn read_load() -> f64 {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0)
}

fn sample_freqs(cpus: &[usize]) -> Vec<u64> {
    cpus.iter()
        .filter_map(|cpu| {
            fs::read_to_string(format!(
                "/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq"
            ))
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()
            .map(|khz| khz / 1000)
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn pin_thread(cpu: usize) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        if cpu < libc::CPU_SETSIZE as usize {
            libc::CPU_SET(cpu, &mut set);
            libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn pin_thread(_cpu: usize) {}

#[cfg(target_os = "linux")]
fn current_cpu() -> i32 {
    unsafe { libc::sched_getcpu() }
}

#[cfg(not(target_os = "linux"))]
fn current_cpu() -> i32 {
    -1
}

#[cfg(target_os = "linux")]
fn thread_cpu_time() -> Duration {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut value);
    }
    Duration::new(value.tv_sec as u64, value.tv_nsec as u32)
}

#[cfg(not(target_os = "linux"))]
fn thread_cpu_time() -> Duration {
    Duration::ZERO
}

fn parse_cpu_list(value: &str) -> Result<Vec<usize>, String> {
    let mut cpus = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let start: usize = start
                .parse()
                .map_err(|_| format!("invalid CPU range {part:?}"))?;
            let end: usize = end
                .parse()
                .map_err(|_| format!("invalid CPU range {part:?}"))?;
            if start > end {
                return Err(format!("invalid CPU range {part:?}"));
            }
            cpus.extend(start..=end);
        } else {
            cpus.push(part.parse().map_err(|_| format!("invalid CPU {part:?}"))?);
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    Ok(cpus)
}

struct CorpusShape {
    posts: usize,
    utf8_bytes: usize,
    units: usize,
    kind_posts: [usize; 3],
    kind_units: [usize; 3],
    expected_mentions: usize,
    expected_posts_with: usize,
}

#[derive(Serialize)]
struct BenchReport {
    mode: String,
    pattern: String,
    corpus: String,
    corpus_sha256: Option<String>,
    limit: Option<usize>,
    threads: usize,
    repeat: usize,
    warmup: usize,
    warmup_seconds: f64,
    min_efficiency: f64,
    retries: usize,
    cpu_list: Vec<usize>,
    worker_cores: Vec<i32>,
    worker_efficiencies: Vec<f64>,
    posts: usize,
    utf8_bytes: usize,
    units: usize,
    ucs1_posts: usize,
    ucs2_posts: usize,
    ucs4_posts: usize,
    ucs1_units: usize,
    ucs2_units: usize,
    ucs4_units: usize,
    count: usize,
    count_kind: String,
    posts_with_count: usize,
    expected_mentions: usize,
    expected_posts_with_mentions: usize,
    first_s: f64,
    runs_s: Vec<f64>,
    first_cpu_s: f64,
    runs_cpu_s: Vec<f64>,
    cpu_efficiency: f64,
    median_s: f64,
    min_s: f64,
    ns_per_post: f64,
    mb_per_s: f64,
    load_before: f64,
    load_after: f64,
    freq_before_mhz: Vec<u64>,
    freq_after_mhz: Vec<u64>,
    cpu: String,
    os: String,
    rustc: String,
}

fn build_report(
    opts: &Options,
    corpus_path: &Path,
    corpus_sha256: Option<String>,
    mode: Mode,
    shape: CorpusShape,
    runs: &[Trial],
    env: &EnvSample,
) -> BenchReport {
    let seconds: Vec<f64> = runs.iter().map(|run| run.elapsed.as_secs_f64()).collect();
    let cpu_seconds: Vec<f64> = runs.iter().map(|run| run.cpu.as_secs_f64()).collect();
    let first_s = seconds[0];
    let median_s = median(&seconds);
    let min_s = seconds.iter().copied().fold(f64::INFINITY, f64::min);
    let posts = shape.posts;
    let ns_per_post = if posts == 0 {
        0.0
    } else {
        median_s * 1e9 / posts as f64
    };
    let mb_per_s = if median_s > 0.0 {
        shape.utf8_bytes as f64 / 1e6 / median_s
    } else {
        0.0
    };
    let first_cpu_s = cpu_seconds[0];
    let cpu_efficiency = if first_s > 0.0 {
        first_cpu_s / (first_s * env.threads as f64)
    } else {
        0.0
    };
    BenchReport {
        mode: mode.as_str().to_string(),
        pattern: opts.pattern.clone(),
        corpus: corpus_path.display().to_string(),
        corpus_sha256,
        limit: opts.limit,
        threads: env.threads,
        repeat: opts.repeat,
        warmup: opts.warmup,
        warmup_seconds: opts.warmup_seconds,
        min_efficiency: opts.min_efficiency,
        retries: opts.retries,
        cpu_list: opts.cpu_list.clone(),
        worker_cores: runs[0].cores.clone(),
        worker_efficiencies: runs[0].efficiencies.clone(),
        posts,
        utf8_bytes: shape.utf8_bytes,
        units: shape.units,
        ucs1_posts: shape.kind_posts[0],
        ucs2_posts: shape.kind_posts[1],
        ucs4_posts: shape.kind_posts[2],
        ucs1_units: shape.kind_units[0],
        ucs2_units: shape.kind_units[1],
        ucs4_units: shape.kind_units[2],
        count: runs[0].count,
        count_kind: "mentions".to_string(),
        posts_with_count: runs[0].posts_with,
        expected_mentions: shape.expected_mentions,
        expected_posts_with_mentions: shape.expected_posts_with,
        first_s,
        runs_s: seconds,
        first_cpu_s,
        runs_cpu_s: cpu_seconds,
        cpu_efficiency,
        median_s,
        min_s,
        ns_per_post,
        mb_per_s,
        load_before: env.load_before,
        load_after: env.load_after,
        freq_before_mhz: env.freq_before_mhz.clone(),
        freq_after_mhz: env.freq_after_mhz.clone(),
        cpu: cpu_name(),
        os: std::env::consts::OS.to_string(),
        rustc: rustc_version(),
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn cpu_name() -> String {
    if let Ok(info) = fs::read_to_string("/proc/cpuinfo") {
        for line in info.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                if let Some((_, value)) = rest.split_once(':') {
                    return value.trim().to_string();
                }
            }
        }
    }
    std::env::consts::ARCH.to_string()
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn self_test() -> Result<(), String> {
    let ucs_corpus = load_ucs_from_reader(Cursor::new(SELF_TEST_JSONL), None)?;
    let bytes_corpus = load_bytes_from_reader(Cursor::new(SELF_TEST_JSONL), None)?;
    if ucs_corpus.expected_mentions != 7 {
        return Err(format!(
            "self-test naive mentions {} != 7",
            ucs_corpus.expected_mentions
        ));
    }
    if ucs_corpus.expected_posts_with != 6 {
        return Err(format!(
            "self-test naive posts {} != 6",
            ucs_corpus.expected_posts_with
        ));
    }
    if ucs_corpus.kind_posts[1] == 0 || ucs_corpus.kind_posts[2] == 0 {
        return Err("self-test must cover UCS-2 and UCS-4".to_string());
    }

    let re = Regex::new(DEFAULT_PATTERN).map_err(|e| format!("pattern: {e}"))?;
    let mut scratches = vec![re.scratch(), re.scratch()];
    let ucs = scan_ucs(&re, &ucs_corpus, &mut scratches, &[]);
    if ucs.count != 7 || ucs.posts_with != 6 {
        return Err(format!(
            "self-test ucs count {} posts {}",
            ucs.count, ucs.posts_with
        ));
    }

    let bytes_re =
        regex::bytes::Regex::new(DEFAULT_PATTERN).map_err(|e| format!("pattern: {e}"))?;
    let bytes = scan_bytes(&bytes_re, &bytes_corpus, 2, &[]);
    if bytes.count != 7 || bytes.posts_with != 6 {
        return Err(format!(
            "self-test bytes count {} posts {}",
            bytes.count, bytes.posts_with
        ));
    }

    let str_re = regex::Regex::new(DEFAULT_PATTERN).map_err(|e| format!("pattern: {e}"))?;
    let text = scan_str(&str_re, &bytes_corpus, 2, &[]);
    if text.count != 7 || text.posts_with != 6 {
        return Err(format!(
            "self-test str count {} posts {}",
            text.count, text.posts_with
        ));
    }

    println!(
        "self-test ok: {} mentions in {} posts",
        ucs.count, ucs.posts_with
    );
    Ok(())
}
