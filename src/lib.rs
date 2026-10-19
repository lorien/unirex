//! Experimental linear-time regular-expression engine over UCS haystacks.
//!
//! Milestone 1 supports a strict Python-dialect subset on UCS-1, UCS-2, and
//! UCS-4 slices. Patterns are parsed with `regex-syntax`'s AST and lowered by
//! this crate; unsupported syntax is rejected rather than reinterpreted with
//! Rust `regex` semantics.

mod compile;
mod dfa;
mod error;
mod pikevm;
#[cfg(feature = "python")]
mod python;
mod ucs;
mod unicode_tables;

pub use error::Error;
pub use pikevm::Scratch;
pub use ucs::{Ucs, UcsKind};

use compile::Program;

/// A match span in code units.
///
/// Every UCS unit is one code point, so offsets are also code-point offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Match {
    start: usize,
    end: usize,
}

impl Match {
    /// The start offset, in code units.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The end offset, in code units.
    pub fn end(&self) -> usize {
        self.end
    }

    /// The matched length, in code units.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the match is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The match span.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }

    pub(crate) fn new(start: usize, end: usize) -> Match {
        Match { start, end }
    }
}

/// A compiled pattern.
pub struct Regex {
    prog: Program,
}

impl Regex {
    /// Compile `pattern` using the supported Python-dialect subset.
    ///
    /// Returns [`Error::Unsupported`] for syntax this milestone does not
    /// implement, including `$`, `\w`, `\b`, and the `i m s x` flags.
    pub fn new(pattern: &str) -> Result<Regex, Error> {
        compile::compile(pattern).map(|prog| Regex { prog })
    }

    /// Allocate reusable search scratch for this pattern.
    ///
    /// Reuse one `Scratch` across searches to avoid per-search allocation.
    /// A `Scratch` is not shared between threads; give each worker its own.
    pub fn scratch(&self) -> Scratch {
        Scratch::new(self.prog.insts.len())
    }

    /// Number of capture groups, excluding group 0.
    pub fn group_count(&self) -> usize {
        self.prog.groups
    }

    /// Fill `slots` with the capture spans of `m`.
    ///
    /// `slots` must hold `2 * (group_count() + 1)` entries and is interpreted as
    /// `[start0, end0, start1, end1, ...]`. Unmatched groups receive
    /// [`usize::MAX`] for both of their slots.
    ///
    /// The DFA finds the match span on the hot path; this pass re-derives group
    /// spans by running the PikeVM over `m` only, never over the whole haystack.
    /// Returns `false` when the span cannot be re-derived, which callers must
    /// treat as "groups unavailable" rather than as a pattern error.
    pub fn captures(
        &self,
        scratch: &mut Scratch,
        hay: Ucs<'_>,
        m: Match,
        slots: &mut [usize],
    ) -> bool {
        match hay {
            Ucs::Ucs1(units) => {
                pikevm::capture_slots(&self.prog, scratch, units, m.start, m.end, slots)
            }
            Ucs::Ucs2(units) => {
                pikevm::capture_slots(&self.prog, scratch, units, m.start, m.end, slots)
            }
            Ucs::Ucs4(units) => {
                pikevm::capture_slots(&self.prog, scratch, units, m.start, m.end, slots)
            }
        }
    }

    /// Whether the pattern matches anywhere in `hay`.
    pub fn is_match(&self, hay: Ucs<'_>) -> bool {
        self.find(hay).is_some()
    }

    /// Whether the pattern matches anywhere in `hay`, using `scratch`.
    pub fn is_match_with(&self, scratch: &mut Scratch, hay: Ucs<'_>) -> bool {
        self.find_with(scratch, hay).is_some()
    }

    /// Find the leftmost match in `hay`.
    pub fn find(&self, hay: Ucs<'_>) -> Option<Match> {
        let mut scratch = self.scratch();
        self.find_with(&mut scratch, hay)
    }

    /// Find the leftmost match in `hay`, using `scratch`.
    pub fn find_with(&self, scratch: &mut Scratch, hay: Ucs<'_>) -> Option<Match> {
        self.find_from_with(scratch, hay, 0, None)
    }

    /// Iterate over non-overlapping matches using Python 3.7+ empty-match
    /// iteration.
    pub fn find_iter<'r, 'h>(&'r self, hay: Ucs<'h>) -> Matches<'r, 'h> {
        Matches {
            re: self,
            scratch: self.scratch(),
            hay,
            pos: 0,
            retry_nonempty_at: None,
            done: false,
        }
    }

    /// Iterate over non-overlapping matches using a reusable `Scratch`.
    pub fn find_iter_with<'r, 's, 'h>(
        &'r self,
        scratch: &'s mut Scratch,
        hay: Ucs<'h>,
    ) -> MatchesWith<'r, 's, 'h> {
        MatchesWith {
            re: self,
            scratch,
            hay,
            pos: 0,
            retry_nonempty_at: None,
            done: false,
        }
    }

    fn find_from_with(
        &self,
        scratch: &mut Scratch,
        hay: Ucs<'_>,
        start: usize,
        retry_nonempty_at: Option<usize>,
    ) -> Option<Match> {
        match hay {
            Ucs::Ucs1(units) => pikevm::find(&self.prog, scratch, units, start, retry_nonempty_at),
            Ucs::Ucs2(units) => pikevm::find(&self.prog, scratch, units, start, retry_nonempty_at),
            Ucs::Ucs4(units) => pikevm::find(&self.prog, scratch, units, start, retry_nonempty_at),
        }
    }
}

fn advance(
    re: &Regex,
    scratch: &mut Scratch,
    hay: Ucs<'_>,
    pos: &mut usize,
    retry_nonempty_at: &mut Option<usize>,
) -> Option<Match> {
    let m = re.find_from_with(scratch, hay, *pos, *retry_nonempty_at)?;
    if m.is_empty() {
        *pos = m.end();
        *retry_nonempty_at = Some(m.end());
    } else {
        *pos = m.end();
        *retry_nonempty_at = None;
    }
    Some(m)
}

/// Iterator over matches returned by [`Regex::find_iter`].
pub struct Matches<'r, 'h> {
    re: &'r Regex,
    scratch: Scratch,
    hay: Ucs<'h>,
    pos: usize,
    retry_nonempty_at: Option<usize>,
    done: bool,
}

impl Iterator for Matches<'_, '_> {
    type Item = Match;

    fn next(&mut self) -> Option<Match> {
        if self.done {
            return None;
        }
        match advance(
            self.re,
            &mut self.scratch,
            self.hay,
            &mut self.pos,
            &mut self.retry_nonempty_at,
        ) {
            Some(m) => Some(m),
            None => {
                self.done = true;
                None
            }
        }
    }
}

impl std::iter::FusedIterator for Matches<'_, '_> {}

/// Iterator over matches returned by [`Regex::find_iter_with`].
pub struct MatchesWith<'r, 's, 'h> {
    re: &'r Regex,
    scratch: &'s mut Scratch,
    hay: Ucs<'h>,
    pos: usize,
    retry_nonempty_at: Option<usize>,
    done: bool,
}

impl Iterator for MatchesWith<'_, '_, '_> {
    type Item = Match;

    fn next(&mut self) -> Option<Match> {
        if self.done {
            return None;
        }
        match advance(
            self.re,
            &mut *self.scratch,
            self.hay,
            &mut self.pos,
            &mut self.retry_nonempty_at,
        ) {
            Some(m) => Some(m),
            None => {
                self.done = true;
                None
            }
        }
    }
}

impl std::iter::FusedIterator for MatchesWith<'_, '_, '_> {}
