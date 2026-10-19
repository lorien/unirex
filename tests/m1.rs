use std::time::{Duration, Instant};
use unirex::{Error, Match, Regex, Ucs};

fn spans(iter: impl Iterator<Item = Match>) -> Vec<(usize, usize)> {
    iter.map(|m| (m.start(), m.end())).collect()
}

fn find1(pattern: &str, hay: &str) -> Option<(usize, usize)> {
    let re = Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern:?}: {e}"));
    let units: Vec<u8> = hay.bytes().collect();
    re.find(Ucs::Ucs1(&units)).map(|m| (m.start(), m.end()))
}

fn iter1(pattern: &str, hay: &str) -> Vec<(usize, usize)> {
    let re = Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern:?}: {e}"));
    let units: Vec<u8> = hay.bytes().collect();
    spans(re.find_iter(Ucs::Ucs1(&units)))
}

fn assert_rejected(pattern: &str) {
    match Regex::new(pattern) {
        Ok(_) => panic!("expected {pattern:?} to be rejected"),
        Err(Error::Unsupported { .. }) | Err(Error::Parse(_)) => {}
        Err(err) => panic!("unexpected error for {pattern:?}: {err}"),
    }
}

fn assert_unsupported(pattern: &str) {
    match Regex::new(pattern) {
        Err(Error::Unsupported { .. }) => {}
        Ok(_) => panic!("expected {pattern:?} to be unsupported"),
        Err(err) => panic!("expected Unsupported for {pattern:?}, got {err}"),
    }
}

#[test]
fn literals_and_alternation() {
    assert_eq!(find1("abc", "xxabcxx"), Some((2, 5)));
    assert_eq!(find1("a|ab", "ab"), Some((0, 1)));
    assert_eq!(find1("ab|a", "ab"), Some((0, 2)));
    assert_eq!(find1("(a|ab)c", "abc"), Some((0, 3)));
    assert_eq!(find1("foo|bar|baz", "xxbaz"), Some((2, 5)));
}

#[test]
fn dot_and_classes() {
    assert_eq!(find1(".", "a"), Some((0, 1)));
    assert_eq!(find1(".", "\n"), None);
    assert_eq!(find1("[a-c]+", "xxabcxx"), Some((2, 5)));
    assert_eq!(find1("[^a-c]+", "xxabcxx"), Some((0, 2)));
    assert_eq!(find1("[a&b]+", "a&b"), Some((0, 3)));
    assert_eq!(find1("[a~b]+", "a~b"), Some((0, 3)));
    assert_eq!(find1("[abc]", "zzbzz"), Some((2, 3)));
    assert_eq!(find1("[$]", "$"), Some((0, 1)));
    assert_eq!(find1("[.]", "."), Some((0, 1)));
    assert_eq!(find1("[*]", "*"), Some((0, 1)));
}

#[test]
fn greedy_repetition() {
    assert_eq!(find1("a*", "aaa"), Some((0, 3)));
    assert_eq!(find1("a+", "aaa"), Some((0, 3)));
    assert_eq!(find1("a?", "aaa"), Some((0, 1)));
    assert_eq!(find1("a{2}", "aaaa"), Some((0, 2)));
    assert_eq!(find1("a{2,3}", "aaaa"), Some((0, 3)));
    assert_eq!(find1("a{2,}", "aaaa"), Some((0, 4)));
    assert_eq!(find1("a+b", "aaab"), Some((0, 4)));
    assert_eq!(find1("(ab)+", "ababx"), Some((0, 4)));
    assert_eq!(find1("(?:ab|cd)+", "abcd"), Some((0, 4)));
    assert_eq!(find1("(a*)*b", "aaab"), Some((0, 4)));
}

#[test]
fn anchors() {
    assert_eq!(find1("^abc", "abc"), Some((0, 3)));
    assert_eq!(find1("^abc", "xabc"), None);
    assert_eq!(find1(r"\Aabc", "abc"), Some((0, 3)));
    assert_eq!(find1(r"\Aabc", "xabc"), None);
    assert_eq!(find1(r"abc\Z", "abc"), Some((0, 3)));
    assert_eq!(find1(r"abc\Z", "abcd"), None);
    assert_eq!(find1(r"abc\Z", "abc\n"), None);
    assert_eq!(find1(r"abc\z", "abc"), Some((0, 3)));
    assert_eq!(find1(r"\z", "abc"), Some((3, 3)));
}

#[test]
fn python_numeric_escapes() {
    assert_eq!(find1(r"\x41", "A"), Some((0, 1)));
    assert_eq!(find1(r"\u0041", "A"), Some((0, 1)));
    assert_eq!(find1(r"\U00000041", "A"), Some((0, 1)));
    assert_eq!(find1(r"\101", "A"), Some((0, 1)));
    assert_eq!(find1(r"\111", "I"), Some((0, 1)));
    assert_eq!(find1(r"\0", "\0"), Some((0, 1)));
    assert_eq!(find1(r"\011", "\t"), Some((0, 1)));
    assert_eq!(find1(r"\08", "\u{0}8"), Some((0, 2)));
    assert_eq!(find1(r"[\1]", "\u{1}"), Some((0, 1)));
    assert_eq!(find1(r"[\11]", "\t"), Some((0, 1)));
    assert_eq!(find1(r"[\111]", "I"), Some((0, 1)));
}

#[test]
fn python_empty_match_iteration() {
    assert_eq!(iter1("x*", "ab"), vec![(0, 0), (1, 1), (2, 2)]);
    assert_eq!(iter1("|a", "a"), vec![(0, 0), (0, 1), (1, 1)]);
    assert_eq!(iter1("a|", "a"), vec![(0, 1), (1, 1)]);
    assert_eq!(iter1("a*", "aaa"), vec![(0, 3), (3, 3)]);
    assert_eq!(iter1("a*", "baa"), vec![(0, 0), (1, 3), (3, 3)]);
    assert_eq!(iter1("", ""), vec![(0, 0)]);
    assert_eq!(iter1(r"\Z", "a\n"), vec![(2, 2)]);
    assert_eq!(iter1(r"\Z", "a"), vec![(1, 1)]);
}

/// Spans produced by CPython `re` for the same patterns and haystacks.
/// The candidate loop must not be quadratic: `a+b` over a run of `a` used to
/// restart a full anchored scan per prefilter hit.
///
/// Timing assertions only make sense in a release build; the debug build skips
/// this, as `spec/docs/testing.md` requires.
#[test]
fn search_time_is_linear_in_haystack_length() {
    if cfg!(debug_assertions) {
        return;
    }
    fn best(pattern: &str, hay: &str) -> Duration {
        let re = Regex::new(pattern).unwrap();
        let units: Vec<u8> = hay.bytes().collect();
        let mut scratch = re.scratch();
        let mut fastest = Duration::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            let found = re.find_with(&mut scratch, Ucs::Ucs1(&units));
            let elapsed = start.elapsed();
            assert_eq!(found, None, "expected no match for {pattern:?}");
            fastest = fastest.min(elapsed);
        }
        fastest
    }

    for pattern in ["a+b", "a+b$", "(a|aa)+b"] {
        let small = best(pattern, &"a".repeat(8_000));
        let large = best(pattern, &"a".repeat(32_000));
        // Four times the input; linear work grows by about four, quadratic by 16.
        assert!(
            large < small * 8,
            "pattern {pattern:?} looks superlinear: {small:?} -> {large:?}"
        );
    }
}

#[test]
fn python_classes_boundaries_and_end_anchor() {
    type Case = (&'static str, &'static str, &'static [(usize, usize)]);
    let cases: &[Case] = &[
        (r"\w+", "asd 123 _x", &[(0, 3), (4, 7), (8, 10)]),
        (r"\s+", "a b\tc", &[(1, 2), (3, 4)]),
        (r"\W+", "a-b_c!", &[(1, 2), (5, 6)]),
        (r"[\w.]+", "a.b c", &[(0, 3), (4, 5)]),
        (r"[^\w]+", "a.b c", &[(1, 2), (3, 4)]),
        (r"\bfoo\b", "foo foobar foo", &[(0, 3), (11, 14)]),
        (r"\Bfoo", "xfoo foo", &[(1, 4)]),
        (r"foo$", "foo\n", &[(0, 3)]),
        (r"foo$", "foobar\n", &[]),
        (r"\b", "ab cd", &[(0, 0), (2, 2), (3, 3), (5, 5)]),
        (r"\s\b", "a b", &[(1, 2)]),
        // CPython never reports \B in an empty haystack.
        (r"\B", "", &[]),
        (r"\B", " ", &[(0, 0), (1, 1)]),
    ];
    for (pattern, hay, expected) in cases {
        assert_eq!(
            &iter1(pattern, hay)[..],
            *expected,
            "pattern {pattern:?} haystack {hay:?}"
        );
    }
}

/// Spans produced by CPython `re` for the same patterns and haystacks.
#[test]
fn python_flags_lazy_repetition_and_named_groups() {
    type Case = (&'static str, &'static str, &'static [(usize, usize)]);
    let cases: &[Case] = &[
        (r"(?i)abc", "ABC abc", &[(0, 3), (4, 7)]),
        (r"(?i)[^a-c]+", "ABCxyz", &[(3, 6)]),
        (r"(?m)^foo", "x\nfoo", &[(2, 5)]),
        (r"(?m)foo$", "foo\nfoo", &[(0, 3), (4, 7)]),
        (r"(?s)a.b", "a\nb", &[(0, 3)]),
        (r"(?i:a)b", "Ab", &[(0, 2)]),
        (r"(?i:a)b", "AB", &[]),
        (
            r"(?P<user>[a-z]+)@(?P<host>[a-z.]+)",
            "me@example.com",
            &[(0, 14)],
        ),
    ];
    for (pattern, hay, expected) in cases {
        assert_eq!(
            &iter1(pattern, hay)[..],
            *expected,
            "pattern {pattern:?} haystack {hay:?}"
        );
    }

    // Lazy repetition changes group spans, not the whole match.
    let re = Regex::new(r"(a+?)(a+)").unwrap();
    let units: Vec<u8> = b"aaa".to_vec();
    let found = re.find(Ucs::Ucs1(&units)).unwrap();
    let mut slots = vec![usize::MAX; 2 * (re.group_count() + 1)];
    let mut scratch = re.scratch();
    assert!(re.captures(&mut scratch, Ucs::Ucs1(&units), found, &mut slots));
    assert_eq!(&slots[2..], &[0, 1, 1, 3]);

    let named = Regex::new(r"(?P<user>[a-z]+)").unwrap();
    assert_eq!(named.capture_names().collect::<Vec<_>>(), vec![("user", 1)]);
}

#[test]
fn python_classes_work_on_wide_haystacks() {
    let digits = Regex::new(r"\d+").unwrap();
    let arabic2: Vec<u16> = "abc 123 \u{664}\u{665}".encode_utf16().collect();
    assert_eq!(
        spans(digits.find_iter(Ucs::Ucs2(&arabic2))),
        vec![(4, 7), (8, 10)]
    );
    let arabic4: Vec<u32> = "abc 123 \u{664}\u{665}".chars().map(u32::from).collect();
    assert_eq!(
        spans(digits.find_iter(Ucs::Ucs4(&arabic4))),
        vec![(4, 7), (8, 10)]
    );

    let re = Regex::new(r"\w+").unwrap();
    let units2: Vec<u16> = "Привет мир".encode_utf16().collect();
    assert_eq!(
        spans(re.find_iter(Ucs::Ucs2(&units2))),
        vec![(0, 6), (7, 10)]
    );
    let units4: Vec<u32> = "Привет мир".chars().map(u32::from).collect();
    assert_eq!(
        spans(re.find_iter(Ucs::Ucs4(&units4))),
        vec![(0, 6), (7, 10)]
    );
}

#[test]
fn rejects_rust_dialect_and_unimplemented_python() {
    for pattern in [
        "[a&&b]",
        "[a--b]",
        "[a~~b]",
        "[[:alpha:]]",
        "[a[b]]",
        r"\p{L}",
        r"\P{L}",
        r"\x{41}",
        "(?<x>a)",
        r"\b{start}",
        r"\<",
        r"\>",
        r"\1",
        r"\11",
        r"\8",
        r"\400",
        r"[\400]",
        r"\N{LATIN CAPITAL LETTER A}",
        "(?=a)",
        "(a)\\1",
        "^*",
        r"\A*",
        "^+",
        r"\z?",
        r"[\Z]",
    ] {
        assert_rejected(pattern);
    }

    for pattern in ["[a&&b]", r"\p{L}", "(?<x>a)"] {
        assert_unsupported(pattern);
    }
}

#[test]
fn class_edges_match_python() {
    assert_eq!(find1("[]]", "]"), Some((0, 1)));
    assert_eq!(find1("[^]]", "a"), Some((0, 1)));
    assert_eq!(find1("[^]]", "]"), None);
    assert_eq!(find1("[a-]", "-a"), Some((0, 1)));
    assert_eq!(find1("[-a]", "-a"), Some((0, 1)));
    assert_eq!(find1(r"(?:\A)*", "a"), Some((0, 0)));
}

#[test]
fn ucs_kinds_produce_the_same_match() {
    let pattern = "β+";
    let re = Regex::new(pattern).unwrap();

    let two: Vec<u16> = "aββc".chars().map(|c| c as u32 as u16).collect();
    let four: Vec<u32> = "aββc".chars().map(|c| c as u32).collect();
    assert_eq!(
        re.find(Ucs::Ucs2(&two)).map(|m| (m.start(), m.end())),
        Some((1, 3))
    );
    assert_eq!(
        re.find(Ucs::Ucs4(&four)).map(|m| (m.start(), m.end())),
        Some((1, 3))
    );

    let astral: Vec<u32> = "a𐐷b".chars().map(|c| c as u32).collect();
    let re_astral = Regex::new("𐐷").unwrap();
    assert_eq!(
        re_astral
            .find(Ucs::Ucs4(&astral))
            .map(|m| (m.start(), m.end())),
        Some((1, 2))
    );
}

#[test]
fn ucs2_lone_surrogates_are_units() {
    let units = [b'a' as u16, 0xD800, b'b' as u16];
    let hay = Ucs::Ucs2(&units);
    let dot = Regex::new(".").unwrap();
    assert_eq!(dot.find(hay).map(|m| (m.start(), m.end())), Some((0, 1)));
    let middle = Regex::new("a.b").unwrap();
    assert_eq!(middle.find(hay).map(|m| (m.start(), m.end())), Some((0, 3)));
    let non_a = Regex::new("[^a]").unwrap();
    assert_eq!(non_a.find(hay).map(|m| (m.start(), m.end())), Some((1, 2)));
}

fn assert_same_as_rust_regex(pattern: &str, hay: &str) {
    let ours = Regex::new(pattern).unwrap();
    let reference = regex::Regex::new(pattern).unwrap();
    let units: Vec<u8> = hay.bytes().collect();
    let ours_match = ours.find(Ucs::Ucs1(&units)).map(|m| (m.start(), m.end()));
    let reference_match = reference.find(hay).map(|m| (m.start(), m.end()));
    assert_eq!(
        ours_match, reference_match,
        "pattern {pattern:?} on {hay:?}"
    );
    assert_eq!(
        ours.is_match(Ucs::Ucs1(&units)),
        reference.is_match(hay),
        "is_match pattern {pattern:?} on {hay:?}"
    );
}

#[test]
fn differential_against_rust_regex_on_the_common_subset() {
    let patterns = [
        "abc", "a|ab", "ab|a", "(ab)+", "(?:ab)+", "a{2,3}", "a{2,}", "a{2}", "[a-c]+", "[^a-c]+",
        "^abc", r"\Aabc", r"abc\z", "a.c", "(a|b)c", "a*b", "a?b", "a+b", "(a*)*b",
    ];
    let haystacks = [
        "", "abc", "xxabcxx", "abab", "aaaa", "ac", "bc", "a\nb", "aaab",
    ];
    for pattern in patterns {
        for hay in haystacks {
            assert_same_as_rust_regex(pattern, hay);
        }
    }
}

#[test]
fn adversarial_patterns_terminate() {
    let hay = vec![b'a'; 500];
    let re = Regex::new("(a+)+b").unwrap();
    let start = Instant::now();
    assert!(!re.is_match(Ucs::Ucs1(&hay)));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "(a+)+b took {:?}",
        start.elapsed()
    );

    let hay = vec![b'x'; 500];
    let re = Regex::new("(x+x+)+y").unwrap();
    let start = Instant::now();
    assert!(!re.is_match(Ucs::Ucs1(&hay)));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "(x+x+)+y took {:?}",
        start.elapsed()
    );
}

#[test]
fn scratch_reuse_matches_convenience_api() {
    let re = Regex::new("a*").unwrap();
    let mut scratch = re.scratch();

    for hay in ["baa", "aaa", "xyz", ""] {
        let units: Vec<u8> = hay.bytes().collect();
        let ucs = Ucs::Ucs1(&units);
        let expected: Vec<(usize, usize)> =
            re.find_iter(ucs).map(|m| (m.start(), m.end())).collect();
        let actual: Vec<(usize, usize)> = re
            .find_iter_with(&mut scratch, ucs)
            .map(|m| (m.start(), m.end()))
            .collect();
        assert_eq!(actual, expected, "haystack {hay:?}");
        assert_eq!(
            re.find_with(&mut scratch, ucs)
                .map(|m| (m.start(), m.end())),
            re.find(ucs).map(|m| (m.start(), m.end())),
            "find on {hay:?}"
        );
    }

    let units: Vec<u16> = "aβ".chars().map(|c| c as u16).collect();
    assert_eq!(
        re.find_with(&mut scratch, Ucs::Ucs2(&units))
            .map(|m| (m.start(), m.end())),
        Some((0, 1))
    );
}

#[test]
fn prefilter_covers_optional_prefixes_and_classes() {
    assert_eq!(find1("a?b", "b"), Some((0, 1)));
    assert_eq!(find1("a*b", "b"), Some((0, 1)));
    assert_eq!(find1("(a|)b", "b"), Some((0, 1)));
    assert_eq!(find1("(?:^a|b)", "b"), Some((0, 1)));
    assert_eq!(find1("[a-c]+x", "zzzbx"), Some((3, 5)));
    assert_eq!(find1("ab|cd", "xxcd"), Some((2, 4)));
    assert_eq!(find1("[ab]c", "zzbc"), Some((2, 4)));
    assert_eq!(find1("(a|b)+c", "xxbc"), Some((2, 4)));
    assert_eq!(find1("(a|b)*c", "xxbc"), Some((2, 4)));
    assert_eq!(find1("^a", "xa"), None);
    assert_eq!(find1("a*", "baa"), Some((0, 0)));
    assert_eq!(find1("", "abc"), Some((0, 0)));
    assert_eq!(find1(r"\Z", "abc"), Some((3, 3)));
    assert_eq!(find1("[^a]+", "bbb"), Some((0, 3)));
    assert_eq!(find1(".", "\n"), None);
}

#[test]
fn prefilter_searches_ucs2_and_ucs4_units() {
    let re = Regex::new("@user").unwrap();
    let two: Vec<u16> = "مرحبا @user".chars().map(|c| c as u16).collect();
    assert_eq!(
        re.find(Ucs::Ucs2(&two)).map(|m| (m.start(), m.end())),
        Some((6, 11))
    );
    let four: Vec<u32> = "😀 @user".chars().map(|c| c as u32).collect();
    assert_eq!(
        re.find(Ucs::Ucs4(&four)).map(|m| (m.start(), m.end())),
        Some((2, 7))
    );
}
