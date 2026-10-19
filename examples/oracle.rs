//! Test oracle: read TSV lines of pattern, storage kind, and code points, then
//! print every match with all capture spans. Used by `tools/python_diff.py`.
//!
//! Input per line: `pattern \t kind \t code,code,...` where `kind` is `ucs1`,
//! `ucs2`, or `ucs4`. Output per line: matches separated by `;`, each match as
//! `start,end` for group 0 followed by `start,end` per group, with `-1,-1` for
//! a group that did not participate.

use std::io::{self, BufRead};

use unirex::{Regex, Scratch, Ucs};

fn parse_points(field: &str) -> Vec<u32> {
    field
        .split(',')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().expect("decimal code point"))
        .collect()
}

fn spans(re: &Regex, scratch: &mut Scratch, ucs: Ucs<'_>) -> String {
    let width = 2 * (re.group_count() + 1);
    let mut slots = vec![usize::MAX; width];
    let found: Vec<unirex::Match> = re.find_iter_with(scratch, ucs).collect();
    found
        .iter()
        .map(|found| {
            let mut parts = Vec::with_capacity(width);
            if re.captures(scratch, ucs, *found, &mut slots) {
                parts.push(found.start().to_string());
                parts.push(found.end().to_string());
                for index in 1..(width / 2) {
                    let (lo, hi) = (slots[2 * index], slots[2 * index + 1]);
                    if lo == usize::MAX || hi == usize::MAX {
                        parts.push("-1".to_string());
                        parts.push("-1".to_string());
                    } else {
                        parts.push(lo.to_string());
                        parts.push(hi.to_string());
                    }
                }
            }
            parts.join(",")
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin line");
        let mut fields = line.split('\t');
        let (Some(pattern), Some(kind), Some(points)) =
            (fields.next(), fields.next(), fields.next())
        else {
            println!("BADLINE");
            continue;
        };
        let re = match Regex::new(pattern) {
            Ok(re) => re,
            Err(err) => {
                println!("ERR {err}");
                continue;
            }
        };
        let mut scratch = re.scratch();
        let code_points = parse_points(points);
        let out = match kind {
            "ucs1" => {
                let units: Vec<u8> = code_points.iter().map(|&point| point as u8).collect();
                spans(&re, &mut scratch, Ucs::Ucs1(&units))
            }
            "ucs2" => {
                let units: Vec<u16> = code_points.iter().map(|&point| point as u16).collect();
                spans(&re, &mut scratch, Ucs::Ucs2(&units))
            }
            "ucs4" => {
                let units: Vec<u32> = code_points.clone();
                spans(&re, &mut scratch, Ucs::Ucs4(&units))
            }
            other => format!("BADKIND {other}"),
        };
        println!("{out}");
    }
}
