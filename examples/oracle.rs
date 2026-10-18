//! Test oracle: read TSV lines of pattern and ASCII haystack, print match
//! spans from `find_iter`. Used by `tools/python_diff.py`.

use std::io::{self, BufRead};

use exp_rurex::{Regex, Ucs};

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        let (pattern, hay) = line.split_once('\t').unwrap();
        match Regex::new(pattern) {
            Ok(re) => {
                let units: Vec<u8> = hay.bytes().collect();
                let spans: Vec<String> = re
                    .find_iter(Ucs::Ucs1(&units))
                    .map(|m| format!("{},{}", m.start(), m.end()))
                    .collect();
                println!("{}", spans.join(";"));
            }
            Err(err) => println!("ERR {err}"),
        }
    }
}
