//! Experimental linear-time regular-expression engine over UCS haystacks.
//!
//! Milestone 1 supports a strict Python-dialect subset on UCS-1, UCS-2, and
//! UCS-4 slices. Patterns are parsed with `regex-syntax`'s AST and lowered by
//! this crate; unsupported syntax is rejected rather than reinterpreted with
//! Rust `regex` semantics.

mod compile;
mod error;
mod pikevm;
mod ucs;

pub use error::Error;
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

    /// Whether the pattern matches anywhere in `hay`.
    pub fn is_match(&self, hay: Ucs<'_>) -> bool {
        self.find(hay).is_some()
    }

    /// Find the leftmost match in `hay`.
    pub fn find(&self, hay: Ucs<'_>) -> Option<Match> {
        self.find_from(hay, 0, None)
    }

    /// Iterate over non-overlapping matches using Python 3.7+ empty-match
    /// iteration.
    pub fn find_iter<'r, 'h>(&'r self, hay: Ucs<'h>) -> Matches<'r, 'h> {
        Matches {
            re: self,
            hay,
            pos: 0,
            retry_nonempty_at: None,
            done: false,
        }
    }

    fn find_from(
        &self,
        hay: Ucs<'_>,
        start: usize,
        retry_nonempty_at: Option<usize>,
    ) -> Option<Match> {
        match hay {
            Ucs::Ucs1(units) => pikevm::find(&self.prog, units, start, retry_nonempty_at),
            Ucs::Ucs2(units) => pikevm::find(&self.prog, units, start, retry_nonempty_at),
            Ucs::Ucs4(units) => pikevm::find(&self.prog, units, start, retry_nonempty_at),
        }
    }
}

/// Iterator over matches returned by [`Regex::find_iter`].
pub struct Matches<'r, 'h> {
    re: &'r Regex,
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
        let m = match self
            .re
            .find_from(self.hay, self.pos, self.retry_nonempty_at)
        {
            Some(m) => m,
            None => {
                self.done = true;
                return None;
            }
        };
        if m.is_empty() {
            self.pos = m.end;
            self.retry_nonempty_at = Some(m.end);
        } else {
            self.pos = m.end;
            self.retry_nonempty_at = None;
        }
        Some(m)
    }
}

impl std::iter::FusedIterator for Matches<'_, '_> {}
