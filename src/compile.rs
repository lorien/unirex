//! Pattern adaptation and lowering.
//!
//! `regex-syntax` provides the AST, but its HIR would impose Rust `regex`
//! semantics. This module validates a strict Python-dialect subset and lowers
//! the AST into the small instruction set used by the PikeVM.

use regex_syntax::ast::{
    self, AssertionKind, Ast, ClassSet, ClassSetItem, GroupKind, LiteralKind, RepetitionKind,
    RepetitionRange, Span,
};

use crate::error::Error;
use crate::ucs::{ByteFinder, Scan, UcsKind, Unit};
use crate::unicode_tables;

const MAX_PATTERN_LEN: usize = 100_000;
const MAX_INSTS: usize = 100_000;
const MAX_REPEAT: u32 = 1_000;
const MAX_CLASSES: usize = 10_000;
const MAX_CLASS_RANGES: usize = 100_000;
const MAX_GROUPS: usize = 1_000;
const MAX_CODEPOINT: u32 = 0x10FFFF;
/// `_sre.MAXGROUPS`: group references at or above this value are rejected.
const MAX_GROUP_REFERENCE: u64 = 1_073_741_823;

/// A compiled instruction program.
pub(crate) struct Program {
    pub(crate) insts: Vec<Inst>,
    pub(crate) classes: Vec<Class>,
    pub(crate) start: usize,
    pub(crate) first: Option<FirstSet>,
    /// Number of capture groups, excluding group 0.
    pub(crate) groups: usize,
    /// The program contains an assertion the DFA cannot evaluate without
    /// surrounding context, so the DFA must decline it.
    pub(crate) uses_context: bool,
    /// Capture names paired with their group index.
    pub(crate) names: Vec<(String, usize)>,
    /// Bytes semantics: ASCII classes and ASCII-only case folding.
    pub(crate) ascii: bool,
    /// Every accepting run passes a start-of-haystack assertion, so an
    /// unanchored search only has to try the first position.
    pub(crate) anchored_start: bool,
}

/// A conservative set of code units that can begin a match.
///
/// `None` in `Program::first` means the prefilter is disabled: the pattern can
/// match empty, starts with a negated class, or has a first set too broad to
/// be useful.
#[derive(Clone, Debug)]
pub(crate) enum FirstSet {
    /// Exactly one code unit, searched with a fast unit scan.
    One {
        target: u32,
        finder16: Option<Box<ByteFinder>>,
        finder32: Option<Box<ByteFinder>>,
    },
    /// A small set of inclusive ranges, searched by a scalar scan.
    Ranges(Vec<(u32, u32)>),
}

impl FirstSet {
    /// Prepare a scan for one UCS width.
    ///
    /// The byte finders are prepared at compile time. The returned scan only
    /// borrows this set for the range case, so it is used for the whole search.
    pub(crate) fn scan<U: Unit>(&self) -> Option<Scan<'_>> {
        match self {
            FirstSet::One {
                target,
                finder16,
                finder32,
            } => match U::KIND {
                UcsKind::Ucs1 => Some(Scan::One(*target)),
                UcsKind::Ucs2 => {
                    if *target > u32::from(u16::MAX) {
                        None
                    } else {
                        finder16
                            .as_deref()
                            .map_or(Some(Scan::One(*target)), |finder| {
                                Some(Scan::Bytes {
                                    target: *target,
                                    finder,
                                })
                            })
                    }
                }
                UcsKind::Ucs4 => finder32
                    .as_deref()
                    .map_or(Some(Scan::One(*target)), |finder| {
                        Some(Scan::Bytes {
                            target: *target,
                            finder,
                        })
                    }),
            },
            FirstSet::Ranges(ranges) => Some(Scan::Ranges(ranges)),
        }
    }
}

/// One NFA instruction.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Inst {
    /// Consume one code point equal to the operand.
    Char(u32),
    /// Consume one code point in the class with this index.
    Class(usize),
    /// Epsilon transition to the first target, then the second.
    Split(usize, usize),
    /// Epsilon transition to one target.
    Jump(usize),
    /// Zero-width assertion.
    Assert(AssertKind),
    /// Record the current position into a capture slot. Epsilon transition.
    Save(usize),
    /// Report a match at the current position.
    Match,
}

/// Assertions supported in milestone 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssertKind {
    /// Start of the haystack.
    Start,
    /// End of the haystack.
    End,
    /// End of the haystack, or just before a final newline (Python `$`).
    EndLine,
    /// Start of the haystack or just after any newline (Python `^` with `m`).
    StartLine,
    /// End of the haystack or just before any newline (Python `$` with `m`).
    EndAnyLine,
    /// Between a word and a non-word unit, or a word unit and the haystack
    /// edge.
    WordBoundary,
    /// Complement of [`AssertKind::WordBoundary`].
    NotWordBoundary,
}

impl AssertKind {
    /// Whether the assertion holds at `pos`, given the unit before it and the
    /// unit at it. `None` means the haystack edge.
    pub(crate) fn holds(
        self,
        prev: Option<u32>,
        next: Option<u32>,
        pos: usize,
        len: usize,
        ascii: bool,
    ) -> bool {
        let is_word = |unit: Option<u32>| match unit {
            Some(unit) if ascii => ascii_is_word(unit),
            Some(unit) => unicode_tables::is_word(unit),
            None => false,
        };
        match self {
            AssertKind::Start => pos == 0,
            AssertKind::End => pos == len,
            AssertKind::EndLine => pos == len || (pos + 1 == len && next == Some('\n' as u32)),
            AssertKind::StartLine => pos == 0 || prev == Some('\n' as u32),
            AssertKind::EndAnyLine => pos == len || next == Some('\n' as u32),
            AssertKind::WordBoundary => is_word(prev) != is_word(next),
            // CPython never reports `\B` in an empty string, even though both
            // sides are trivially non-word there.
            AssertKind::NotWordBoundary => len > 0 && is_word(prev) == is_word(next),
        }
    }
}

/// A character class as a sorted list of inclusive code-point ranges.
#[derive(Debug)]
pub(crate) struct Class {
    ranges: Vec<(u32, u32)>,
    negated: bool,
}

impl Class {
    fn new(ranges: Vec<(u32, u32)>, negated: bool) -> Class {
        let mut ranges = ranges;
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (lo, hi) in ranges {
            if let Some(last) = merged.last_mut() {
                if lo <= last.1.saturating_add(1) {
                    if hi > last.1 {
                        last.1 = hi;
                    }
                    continue;
                }
            }
            merged.push((lo, hi));
        }
        Class {
            ranges: merged,
            negated,
        }
    }

    fn dot() -> Class {
        Class {
            ranges: vec![(0, 9), (11, MAX_CODEPOINT)],
            negated: false,
        }
    }

    pub(crate) fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    pub(crate) fn contains(&self, code_point: u32) -> bool {
        let inside = self
            .ranges
            .binary_search_by(|&(lo, hi)| {
                if code_point < lo {
                    std::cmp::Ordering::Greater
                } else if code_point > hi {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok();
        inside ^ self.negated
    }
}

/// Compile a pattern with the strict Python-dialect subset.
pub(crate) fn compile(pattern: &str) -> Result<Program, Error> {
    compile_with_ascii(pattern, false)
}

/// Compile with `re` bytes semantics: ASCII classes and ASCII case folding.
pub(crate) fn compile_bytes(pattern: &str) -> Result<Program, Error> {
    compile_with_ascii(pattern, true)
}

fn compile_with_ascii(pattern: &str, ascii: bool) -> Result<Program, Error> {
    if pattern.len() > MAX_PATTERN_LEN {
        return Err(Error::TooLarge {
            what: "pattern length",
            offset: 0,
        });
    }

    let normalized = normalize_end_anchor(pattern);
    validate_python_escapes(&normalized)?;
    prevalidate(&normalized, ascii)?;
    let mut parser = ast::parse::ParserBuilder::new().octal(true).build();
    let ast = parser.parse(&normalized)?;
    let mut compiler = Compiler::new(ascii);
    compiler.compile(&ast)?;
    compiler.finish()
}

/// Python spells the end-of-text anchor `\Z`; `regex-syntax` spells it `\z`.
/// Rewrite only unescaped `\Z` sequences, preserving every other byte.
fn normalize_end_anchor(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('Z') => out.push_str("\\z"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Python treats `\1` and `\11` as backreferences outside a character class,
/// but as octal escapes inside one. It also caps octal values at `0o377`.
/// `regex-syntax` with `octal(true)` would silently accept the first form as
/// octal, so reject it before parsing.
fn validate_python_escapes(pattern: &str) -> Result<(), Error> {
    let chars: Vec<(usize, char)> = pattern.char_indices().collect();
    let mut index = 0;
    let mut in_class = false;
    while index < chars.len() {
        let (offset, c) = chars[index];
        match c {
            '[' if !in_class => {
                in_class = true;
                index += 1;
                if index < chars.len() && chars[index].1 == '^' {
                    index += 1;
                }
                if index < chars.len() && chars[index].1 == ']' {
                    index += 1;
                }
            }
            ']' if in_class => {
                in_class = false;
                index += 1;
            }
            '\\' => {
                if index + 1 >= chars.len() {
                    break;
                }
                let next = chars[index + 1].1;
                if !next.is_ascii_digit() {
                    index += 2;
                    continue;
                }
                if next == '8' || next == '9' {
                    return Err(unsupported("bad numeric escape", &span_at(offset)));
                }
                let mut digits = 0usize;
                let mut value = 0u32;
                let mut cursor = index + 1;
                while cursor < chars.len() && digits < 3 {
                    let Some(digit) = chars[cursor].1.to_digit(8) else {
                        break;
                    };
                    value = value * 8 + digit;
                    digits += 1;
                    cursor += 1;
                }
                let python_octal = in_class || next == '0' || digits == 3;
                if !python_octal {
                    return Err(unsupported("backreferences", &span_at(offset)));
                }
                if value > 0o377 {
                    return Err(unsupported("octal escape out of range", &span_at(offset)));
                }
                index = cursor;
            }
            _ => index += 1,
        }
    }
    Ok(())
}

fn span_at(offset: usize) -> Span {
    Span::new(
        ast::Position::new(offset, 0, 0),
        ast::Position::new(offset, 0, 0),
    )
}

/// Active inline flags during lowering.
#[derive(Clone, Copy, Debug, Default)]
struct Flags {
    ignore_case: bool,
    multiline: bool,
    dot_all: bool,
    verbose: bool,
}

struct Compiler {
    ascii: bool,
    flags: Flags,
    /// Inside a `#` comment while verbose mode is active.
    in_comment: bool,
    insts: Vec<Inst>,
    classes: Vec<Class>,
    total_class_ranges: usize,
    groups: usize,
    names: Vec<(String, usize)>,
    /// Set when the program contains an assertion that depends on the
    /// surrounding units, which the lazy DFA cannot evaluate.
    uses_context: bool,
}

impl Compiler {
    fn new(ascii: bool) -> Compiler {
        Compiler {
            ascii,
            flags: Flags::default(),
            in_comment: false,
            insts: Vec::new(),
            classes: Vec::new(),
            total_class_ranges: 0,
            groups: 0,
            names: Vec::new(),
            uses_context: false,
        }
    }

    fn finish(mut self) -> Result<Program, Error> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Error::TooLarge {
                what: "program size",
                offset: 0,
            });
        }
        self.insts.push(Inst::Match);
        let first = first_set(&self.insts, &self.classes, 0);
        let anchored_start = anchored_start(&self.insts);
        Ok(Program {
            insts: self.insts,
            classes: self.classes,
            start: 0,
            first,
            groups: self.groups,
            uses_context: self.uses_context,
            names: self.names,
            ascii: self.ascii,
            anchored_start,
        })
    }

    fn push(&mut self, inst: Inst, span: &Span) -> Result<usize, Error> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Error::TooLarge {
                what: "program size",
                offset: span.start.offset,
            });
        }
        let pc = self.insts.len();
        self.insts.push(inst);
        Ok(pc)
    }

    fn add_class(&mut self, class: Class, span: &Span) -> Result<usize, Error> {
        if self.classes.len() >= MAX_CLASSES {
            return Err(Error::TooLarge {
                what: "class count",
                offset: span.start.offset,
            });
        }
        if self.total_class_ranges.saturating_add(class.ranges.len()) > MAX_CLASS_RANGES {
            return Err(Error::TooLarge {
                what: "class range count",
                offset: span.start.offset,
            });
        }
        self.total_class_ranges += class.ranges.len();
        let id = self.classes.len();
        self.classes.push(class);
        Ok(id)
    }

    fn apply_flags(&mut self, items: &[ast::FlagsItem], span: &Span) -> Result<(), Error> {
        let mut negate = false;
        for item in items {
            match &item.kind {
                ast::FlagsItemKind::Negation => {
                    negate = true;
                    continue;
                }
                ast::FlagsItemKind::Flag(flag) => {
                    let value = !negate;
                    match flag {
                        ast::Flag::CaseInsensitive => self.flags.ignore_case = value,
                        ast::Flag::MultiLine => self.flags.multiline = value,
                        ast::Flag::DotMatchesNewLine => self.flags.dot_all = value,
                        ast::Flag::IgnoreWhitespace => self.flags.verbose = value,
                        _ => return Err(unsupported("this flag", &item.span)),
                    }
                }
            }
        }
        let _ = span;
        Ok(())
    }

    /// Range set closed under case folding, using the generated fold table.
    fn case_ranges(&self, ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
        let mut expanded = ranges.to_vec();
        if self.ascii {
            // Bytes IGNORECASE folds ASCII letters to their opposite case and
            // nothing else.
            for unit in 0x41u32..=0x5A {
                if ranges_contain(ranges, unit) || ranges_contain(ranges, unit + 0x20) {
                    expanded.push((unit, unit));
                    expanded.push((unit + 0x20, unit + 0x20));
                }
            }
            return expanded;
        }
        for &(left, right) in unicode_tables::FOLD {
            if ranges_contain(ranges, left) {
                expanded.push((right, right));
            }
        }
        expanded
    }

    fn set_split_a(&mut self, pc: usize, target: usize) {
        match &mut self.insts[pc] {
            Inst::Split(a, _) => *a = target,
            _ => unreachable!("expected Split instruction"),
        }
    }

    fn set_split_b(&mut self, pc: usize, target: usize) {
        match &mut self.insts[pc] {
            Inst::Split(_, b) => *b = target,
            _ => unreachable!("expected Split instruction"),
        }
    }

    fn compile(&mut self, ast: &Ast) -> Result<(), Error> {
        match ast {
            Ast::Empty(_) => Ok(()),
            Ast::Flags(flags) => {
                // Placement was validated by `prevalidate`; only the flag
                // letters themselves are checked here.
                self.apply_flags(&flags.flags.items, &flags.span)?;
                Ok(())
            }
            Ast::Literal(lit) => {
                check_literal(lit)?;
                let code_point = lit.c as u32;
                // Escaped whitespace such as `\ ` stays literal; only verbatim
                // whitespace is stripped.
                if self.flags.verbose && matches!(lit.kind, LiteralKind::Verbatim) {
                    // Python's verbose mode strips ASCII whitespace and
                    // `#`-to-end-of-line comments, but never inside a class.
                    if matches!(lit.c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
                        return Ok(());
                    }
                    if lit.c == '#' {
                        self.in_comment = true;
                        return Ok(());
                    }
                    if self.in_comment {
                        if lit.c != '\n' {
                            return Ok(());
                        }
                        self.in_comment = false;
                        return Ok(());
                    }
                }
                if self.flags.ignore_case {
                    let class = Class::new(self.case_ranges(&[(code_point, code_point)]), false);
                    let id = self.add_class(class, &lit.span)?;
                    self.push(Inst::Class(id), &lit.span)?;
                } else {
                    self.push(Inst::Char(code_point), &lit.span)?;
                }
                Ok(())
            }
            Ast::Dot(span) => {
                let class = if self.flags.dot_all {
                    Class::new(vec![(0, MAX_CODEPOINT)], false)
                } else {
                    Class::dot()
                };
                let id = self.add_class(class, span)?;
                self.push(Inst::Class(id), span)?;
                Ok(())
            }
            Ast::Assertion(assertion) => match &assertion.kind {
                AssertionKind::StartText => {
                    self.push(Inst::Assert(AssertKind::Start), &assertion.span)?;
                    Ok(())
                }
                AssertionKind::StartLine => {
                    if self.flags.multiline {
                        self.uses_context = true;
                        self.push(Inst::Assert(AssertKind::StartLine), &assertion.span)?;
                    } else {
                        self.push(Inst::Assert(AssertKind::Start), &assertion.span)?;
                    }
                    Ok(())
                }
                AssertionKind::EndText => {
                    self.push(Inst::Assert(AssertKind::End), &assertion.span)?;
                    Ok(())
                }
                AssertionKind::EndLine => {
                    self.uses_context = true;
                    let kind = if self.flags.multiline {
                        AssertKind::EndAnyLine
                    } else {
                        AssertKind::EndLine
                    };
                    self.push(Inst::Assert(kind), &assertion.span)?;
                    Ok(())
                }
                AssertionKind::WordBoundary => {
                    self.uses_context = true;
                    self.push(Inst::Assert(AssertKind::WordBoundary), &assertion.span)?;
                    Ok(())
                }
                AssertionKind::NotWordBoundary => {
                    self.uses_context = true;
                    self.push(Inst::Assert(AssertKind::NotWordBoundary), &assertion.span)?;
                    Ok(())
                }
                _ => Err(unsupported("word-boundary assertions", &assertion.span)),
            },
            Ast::ClassUnicode(class) => Err(unsupported("Unicode property classes", &class.span)),
            Ast::ClassPerl(class) => {
                let ranges = match (&class.kind, self.ascii) {
                    (ast::ClassPerlKind::Word, true) => ASCII_WORD.to_vec(),
                    (ast::ClassPerlKind::Digit, true) => ASCII_DIGIT.to_vec(),
                    (ast::ClassPerlKind::Space, true) => ASCII_SPACE.to_vec(),
                    (ast::ClassPerlKind::Word, false) => unicode_tables::WORD.to_vec(),
                    (ast::ClassPerlKind::Digit, false) => unicode_tables::DIGIT.to_vec(),
                    (ast::ClassPerlKind::Space, false) => unicode_tables::SPACE.to_vec(),
                };
                let mut lowered = Class::new(ranges, class.negated);
                if self.flags.ignore_case {
                    let negated = lowered.negated;
                    lowered = Class::new(self.case_ranges(lowered.ranges()), negated);
                }
                let id = self.add_class(lowered, &class.span)?;
                self.push(Inst::Class(id), &class.span)?;
                Ok(())
            }
            Ast::ClassBracketed(class) => {
                let mut lowered = self.lower_bracketed(class)?;
                if self.flags.ignore_case {
                    // Fold the members and keep the negation, so `[^a-c]`
                    // under `i` still excludes `A`, `B`, and `C`.
                    let negated = lowered.negated;
                    lowered = Class::new(self.case_ranges(lowered.ranges()), negated);
                }
                let id = self.add_class(lowered, &class.span)?;
                self.push(Inst::Class(id), &class.span)?;
                Ok(())
            }
            Ast::Repetition(repetition) => self.compile_repetition(repetition),
            Ast::Group(group) => self.compile_group(group),
            Ast::Alternation(alternation) => {
                self.compile_alternation(&alternation.asts, &alternation.span)
            }
            Ast::Concat(concat) => {
                for sub in &concat.asts {
                    self.compile(sub)?;
                }
                Ok(())
            }
        }
    }

    fn compile_group(&mut self, group: &ast::Group) -> Result<(), Error> {
        self.compile_group_inner(group)
    }

    fn compile_group_inner(&mut self, group: &ast::Group) -> Result<(), Error> {
        match &group.kind {
            GroupKind::CaptureIndex(index) => {
                let index = *index as usize;
                if index > MAX_GROUPS {
                    return Err(Error::TooLarge {
                        what: "capture group count",
                        offset: group.span.start.offset,
                    });
                }
                self.groups = self.groups.max(index);
                self.push(Inst::Save(2 * index), &group.span)?;
                self.compile(&group.ast)?;
                self.push(Inst::Save(2 * index + 1), &group.span)?;
                Ok(())
            }
            GroupKind::NonCapturing(flags) => {
                if flags.items.is_empty() {
                    return self.compile(&group.ast);
                }
                // `(?i:...)` is a scoped flag group, legal anywhere; the
                // placement of the global `(?i)` form is checked by
                // `prevalidate`.
                let saved = self.flags;
                self.apply_flags(&flags.items, &group.span)?;
                let result = self.compile(&group.ast);
                self.flags = saved;
                result
            }
            GroupKind::CaptureName {
                name,
                starts_with_p,
            } => {
                if !starts_with_p {
                    // `(?<name>...)` is Rust syntax; Python spells it `(?P<name>...)`.
                    return Err(unsupported("angle-bracket group syntax", &group.span));
                }
                let index = name.index as usize;
                if index > MAX_GROUPS {
                    return Err(Error::TooLarge {
                        what: "capture group count",
                        offset: group.span.start.offset,
                    });
                }
                let text = name.name.as_str();
                if !is_python_identifier(text) {
                    return Err(unsupported("this capture group name", &group.span));
                }
                if self.names.iter().any(|(existing, _)| existing == text) {
                    return Err(unsupported("duplicate capture group name", &group.span));
                }
                self.names.push((text.to_string(), index));
                self.groups = self.groups.max(index);
                self.push(Inst::Save(2 * index), &group.span)?;
                self.compile(&group.ast)?;
                self.push(Inst::Save(2 * index + 1), &group.span)?;
                Ok(())
            }
        }
    }

    fn compile_repetition(&mut self, repetition: &ast::Repetition) -> Result<(), Error> {
        if matches!(repetition.ast.as_ref(), Ast::Repetition(_)) {
            // Stacked repetition. CPython reports the position just past the
            // innermost operator plus one adjacent lazy or possessive
            // modifier: `x**` at 2, `x*+?` at 3, `x{1,2}+?` at 7.
            // The chain includes the outermost operator itself: regex-syntax
            // merges a lazy marker into an operator's span, so a lazy
            // innermost op already accounts for CPython's one modifier. A
            // greedy innermost op absorbs an adjacent `?` or `+` parent
            // operator, which is CPython's possessive modifier.
            let mut ops: Vec<(&ast::RepetitionOp, bool)> =
                vec![(&repetition.op, repetition.greedy)];
            let mut node = repetition.ast.as_ref();
            while let Ast::Repetition(inner) = node {
                ops.push((&inner.op, inner.greedy));
                node = inner.ast.as_ref();
            }
            let (innermost, innermost_greedy) = *ops.last().expect("nested repetition");
            let mut end = innermost.span.end.offset;
            if innermost_greedy && ops.len() >= 2 {
                let (parent, _) = ops[ops.len() - 2];
                let modifier = matches!(
                    parent.kind,
                    RepetitionKind::ZeroOrOne | RepetitionKind::OneOrMore
                );
                if modifier && parent.span.start.offset == end {
                    end += 1;
                }
            }
            let position = ast::Position::new(end, 1, 1);
            return Err(unsupported(
                "multiple repeat",
                &Span::new(position, position),
            ));
        }
        if matches!(repetition.ast.as_ref(), Ast::Assertion(_)) {
            return Err(unsupported(
                "repetition of an assertion",
                &repetition.op.span,
            ));
        }
        let greedy = repetition.greedy;
        match &repetition.op.kind {
            RepetitionKind::ZeroOrOne => {
                self.compile_zero_or_one(&repetition.ast, &repetition.span, greedy)
            }
            RepetitionKind::ZeroOrMore => {
                self.compile_zero_or_more(&repetition.ast, &repetition.span, greedy)
            }
            RepetitionKind::OneOrMore => {
                self.compile_one_or_more(&repetition.ast, &repetition.span, greedy)
            }
            RepetitionKind::Range(range) => {
                self.compile_range(range, &repetition.ast, &repetition.op.span, greedy)
            }
        }
    }

    fn compile_range(
        &mut self,
        range: &RepetitionRange,
        ast: &Ast,
        span: &Span,
        greedy: bool,
    ) -> Result<(), Error> {
        match range {
            RepetitionRange::Exactly(n) => {
                self.check_repeat(*n, span)?;
                for _ in 0..*n {
                    self.compile(ast)?;
                }
                Ok(())
            }
            RepetitionRange::AtLeast(n) => {
                self.check_repeat(*n, span)?;
                for _ in 0..*n {
                    self.compile(ast)?;
                }
                self.compile_zero_or_more(ast, span, greedy)
            }
            RepetitionRange::Bounded(start, end) => {
                if start > end {
                    return Err(unsupported("invalid repetition range", span));
                }
                self.check_repeat(*end, span)?;
                for _ in 0..*start {
                    self.compile(ast)?;
                }
                for _ in *start..*end {
                    self.compile_zero_or_one(ast, span, greedy)?;
                }
                Ok(())
            }
        }
    }

    fn check_repeat(&self, count: u32, span: &Span) -> Result<(), Error> {
        if count > MAX_REPEAT {
            Err(Error::TooLarge {
                what: "repetition count",
                offset: span.start.offset,
            })
        } else {
            Ok(())
        }
    }

    fn compile_zero_or_one(&mut self, ast: &Ast, span: &Span, greedy: bool) -> Result<(), Error> {
        let skip = if greedy {
            Inst::Split(self.insts.len() + 1, 0)
        } else {
            Inst::Split(0, self.insts.len() + 1)
        };
        let split = self.push(skip, span)?;
        self.compile(ast)?;
        let end = self.insts.len();
        if greedy {
            self.set_split_b(split, end);
        } else {
            self.set_split_a(split, end);
        }
        Ok(())
    }

    fn compile_zero_or_more(&mut self, ast: &Ast, span: &Span, greedy: bool) -> Result<(), Error> {
        let split = self.push(
            if greedy {
                Inst::Split(self.insts.len() + 1, 0)
            } else {
                Inst::Split(0, self.insts.len() + 1)
            },
            span,
        )?;
        self.compile(ast)?;
        self.push(Inst::Jump(split), span)?;
        let end = self.insts.len();
        if greedy {
            self.set_split_b(split, end);
        } else {
            self.set_split_a(split, end);
        }
        Ok(())
    }

    fn compile_one_or_more(&mut self, ast: &Ast, span: &Span, greedy: bool) -> Result<(), Error> {
        let body_start = self.insts.len();
        self.compile(ast)?;
        let split = self.push(
            if greedy {
                Inst::Split(body_start, 0)
            } else {
                Inst::Split(0, body_start)
            },
            span,
        )?;
        let end = self.insts.len();
        if greedy {
            self.set_split_b(split, end);
        } else {
            self.set_split_a(split, end);
        }
        Ok(())
    }

    fn compile_alternation(&mut self, asts: &[Ast], span: &Span) -> Result<(), Error> {
        if asts.is_empty() {
            return Ok(());
        }
        let mut jumps = Vec::new();
        let mut pending_split: Option<usize> = None;
        for (index, ast) in asts.iter().enumerate() {
            if let Some(split) = pending_split.take() {
                self.set_split_b(split, self.insts.len());
            }
            let has_next = index + 1 < asts.len();
            if has_next {
                let next = self.insts.len() + 1;
                let split = self.push(Inst::Split(next, 0), span)?;
                pending_split = Some(split);
            }
            self.compile(ast)?;
            if has_next {
                jumps.push(self.push(Inst::Jump(0), span)?);
            }
        }
        let end = self.insts.len();
        for jump in jumps {
            self.insts[jump] = Inst::Jump(end);
        }
        Ok(())
    }

    fn lower_bracketed(&mut self, class: &ast::ClassBracketed) -> Result<Class, Error> {
        let mut ranges = Vec::new();
        self.lower_class_set(&class.kind, &mut ranges)?;
        Ok(Class::new(ranges, class.negated))
    }

    fn lower_class_set(
        &mut self,
        set: &ClassSet,
        ranges: &mut Vec<(u32, u32)>,
    ) -> Result<(), Error> {
        match set {
            ClassSet::Item(item) => self.lower_class_item(item, ranges),
            ClassSet::BinaryOp(op) => Err(unsupported(
                "character-class set operations (&&, --, ~~)",
                &op.span,
            )),
        }
    }

    fn lower_class_item(
        &mut self,
        item: &ClassSetItem,
        ranges: &mut Vec<(u32, u32)>,
    ) -> Result<(), Error> {
        match item {
            ClassSetItem::Empty(_) => Ok(()),
            ClassSetItem::Literal(lit) => {
                check_literal(lit)?;
                let code_point = lit.c as u32;
                ranges.push((code_point, code_point));
                Ok(())
            }
            ClassSetItem::Range(range) => {
                check_literal(&range.start)?;
                check_literal(&range.end)?;
                let start = range.start.c as u32;
                let end = range.end.c as u32;
                if start > end {
                    return Err(unsupported("invalid character-class range", &range.span));
                }
                ranges.push((start, end));
                Ok(())
            }
            ClassSetItem::Ascii(class) => Err(unsupported("POSIX character classes", &class.span)),
            ClassSetItem::Unicode(class) => {
                Err(unsupported("Unicode property classes", &class.span))
            }
            ClassSetItem::Perl(class) => {
                let table = match (&class.kind, self.ascii) {
                    (ast::ClassPerlKind::Word, true) => ASCII_WORD,
                    (ast::ClassPerlKind::Digit, true) => ASCII_DIGIT,
                    (ast::ClassPerlKind::Space, true) => ASCII_SPACE,
                    (ast::ClassPerlKind::Word, false) => unicode_tables::WORD,
                    (ast::ClassPerlKind::Digit, false) => unicode_tables::DIGIT,
                    (ast::ClassPerlKind::Space, false) => unicode_tables::SPACE,
                };
                if class.negated {
                    // A negated Perl class inside a bracket class contributes
                    // the complement of its ranges.
                    ranges.extend(complement(table));
                } else {
                    ranges.extend_from_slice(table);
                }
                Ok(())
            }
            ClassSetItem::Bracketed(class) => {
                Err(unsupported("nested character classes", &class.span))
            }
            ClassSetItem::Union(union) => {
                for item in &union.items {
                    self.lower_class_item(item, ranges)?;
                }
                Ok(())
            }
        }
    }
}

/// `\\w` for bytes patterns: ASCII letters, digits, and underscore.
const ASCII_WORD: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)];
/// `\\d` for bytes patterns.
const ASCII_DIGIT: &[(u32, u32)] = &[(0x30, 0x39)];
/// `\\s` for bytes patterns.
const ASCII_SPACE: &[(u32, u32)] = &[(0x09, 0x0D), (0x20, 0x20)];

/// Whether a byte value is a word unit under bytes semantics.
fn ascii_is_word(unit: u32) -> bool {
    unit < 0x80 && (unit as u8).is_ascii_alphanumeric() || unit == 0x5F
}

/// Whether `name` is a valid Python capture-group name.
fn is_python_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// Whether a sorted range list contains `code_point`.
fn ranges_contain(ranges: &[(u32, u32)], code_point: u32) -> bool {
    ranges
        .iter()
        .any(|&(lo, hi)| code_point >= lo && code_point <= hi)
}

/// Complement of a sorted range list over the whole code-point space.
fn complement(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(ranges.len() + 1);
    let mut next = 0u32;
    for &(lo, hi) in ranges {
        if lo > next {
            out.push((next, lo - 1));
        }
        next = hi.saturating_add(1);
    }
    if next <= MAX_CODEPOINT {
        out.push((next, MAX_CODEPOINT));
    }
    out
}

const MAX_FIRST_UNITS: u64 = 1024;
const MAX_FIRST_RANGES: usize = 16;

fn first_set(insts: &[Inst], classes: &[Class], start: usize) -> Option<FirstSet> {
    let mut seen = vec![false; insts.len()];
    let mut stack = vec![start];
    let mut ranges: Vec<(u32, u32)> = Vec::new();

    while let Some(pc) = stack.pop() {
        if seen[pc] {
            continue;
        }
        seen[pc] = true;
        match insts[pc] {
            Inst::Split(first, second) => {
                stack.push(first);
                stack.push(second);
            }
            Inst::Jump(target) => stack.push(target),
            Inst::Save(_) => {
                if pc + 1 < insts.len() {
                    stack.push(pc + 1);
                }
            }
            Inst::Assert(_) => {
                if pc + 1 < insts.len() {
                    stack.push(pc + 1);
                }
            }
            Inst::Char(code_point) => ranges.push((code_point, code_point)),
            Inst::Class(class_id) => {
                let class = &classes[class_id];
                if class.negated {
                    return None;
                }
                ranges.extend_from_slice(&class.ranges);
            }
            Inst::Match => return None,
        }
    }

    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (lo, hi) in ranges {
        if let Some(last) = merged.last_mut() {
            if lo <= last.1.saturating_add(1) {
                if hi > last.1 {
                    last.1 = hi;
                }
                continue;
            }
        }
        merged.push((lo, hi));
    }

    let coverage: u64 = merged
        .iter()
        .map(|&(lo, hi)| u64::from(hi) - u64::from(lo) + 1)
        .sum();
    if coverage > MAX_FIRST_UNITS || merged.len() > MAX_FIRST_RANGES {
        return None;
    }
    if merged.len() == 1 && merged[0].0 == merged[0].1 {
        let target = merged[0].0;
        Some(FirstSet::One {
            target,
            finder16: ByteFinder::new(&(target as u16).to_ne_bytes()).map(Box::new),
            finder32: ByteFinder::new(&target.to_ne_bytes()).map(Box::new),
        })
    } else {
        Some(FirstSet::Ranges(merged))
    }
}

fn check_literal(lit: &ast::Literal) -> Result<(), Error> {
    if let LiteralKind::HexBrace(_) = lit.kind {
        return Err(unsupported("brace hex escapes", &lit.span));
    }
    Ok(())
}

fn unsupported(what: &str, span: &Span) -> Error {
    Error::Unsupported {
        what: what.to_string(),
        offset: span.start.offset,
    }
}
/// Every accepting run passes a start-of-haystack assertion, so an
/// unanchored search only needs to try the first position.
///
/// `A[pc]` holds when every path from `pc` to `Match` crosses
/// `Assert(Start)`. Paths through a start assertion are anchored no matter
/// what follows, consuming instructions inherit their successor's verdict,
/// and splits require it on both branches. The fixpoint starts from `false`
/// and only grows, so a few sweeps over the program settle it.
fn anchored_start(insts: &[Inst]) -> bool {
    let mut anchored = vec![false; insts.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for pc in 0..insts.len() {
            if anchored[pc] {
                continue;
            }
            let value = match insts[pc] {
                Inst::Match => continue,
                Inst::Assert(AssertKind::Start) => true,
                Inst::Char(_) | Inst::Class(_) | Inst::Save(_) | Inst::Assert(_) => {
                    anchored[pc + 1]
                }
                Inst::Jump(target) => anchored[target],
                Inst::Split(a, b) => anchored[a] && anchored[b],
            };
            if value {
                anchored[pc] = true;
                changed = true;
            }
        }
    }
    anchored.first().copied().unwrap_or(false)
}

/// Pre-validate the constructs `regex-syntax` cannot parse far enough to
/// report CPython's diagnostics: conditional groups `(?(N)...)` and the
/// placement of global inline flags. The scan mirrors CPython's single
/// left-to-right parse pass, so when two errors compete the earlier one in
/// the pattern wins, as in CPython. Patterns the scan accepts may still be
/// rejected later; the scan only rejects patterns CPython rejects.
fn prevalidate(pattern: &str, bytes: bool) -> Result<(), Error> {
    let chars: Vec<char> = pattern.chars().collect();
    // The scan works in character indices; errors carry byte offsets like
    // every other `Error`, so convert on the way out.
    let to_byte = |char_offset: usize| -> usize {
        chars[..char_offset.min(chars.len())]
            .iter()
            .map(|c| c.len_utf8())
            .sum()
    };
    let mut scan = PreScan {
        bytes,
        groups: 0,
        names: Vec::new(),
        digit_refs: Vec::new(),
        verbose: false,
        consumed: false,
    };
    let scanned = (|| -> Result<(), Error> {
        scan.alternation(&chars, 0, true)?;
        // CPython validates conditional digit references after the whole
        // parse; its `state.groups` counts the implicit group 0, so a
        // reference is valid when it is at most the pattern's group count.
        for (value, offset) in scan.digit_refs.clone() {
            if value > scan.groups as u64 {
                return Err(Error::Unsupported {
                    what: format!("invalid group reference {value}"),
                    offset,
                });
            }
        }
        Ok(())
    })();
    match scanned {
        Ok(()) => Ok(()),
        Err(Error::Unsupported { what, offset }) => Err(Error::Unsupported {
            what,
            offset: to_byte(offset),
        }),
        Err(other) => Err(other),
    }
}

/// State carried across the regions of one [`prevalidate`] scan.
struct PreScan {
    /// The pattern is a `bytes` pattern, so group names are reported
    /// ascii-escaped, as CPython's bytes tokenizer does.
    bytes: bool,
    /// Capture groups opened so far.
    groups: usize,
    /// Capture group names opened so far.
    names: Vec<String>,
    /// Conditional digit references as `(value, name offset)`, checked after
    /// the scan against the final group count.
    digit_refs: Vec<(u64, usize)>,
    /// Global verbose mode is active, so top-level whitespace and `#`
    /// comments are stripped and do not consume a leading flag position.
    verbose: bool,
    /// Something has consumed the pattern's leading position, so a global
    /// inline flag is no longer at the start of the expression.
    consumed: bool,
}

impl PreScan {
    /// Scan an alternation of one or more regions, returning the offset of
    /// the unmatched `)` that closed it, or the end of the pattern.
    ///
    /// With `top`, the regions follow the top-level leading-run rules for
    /// global flags and stripped whitespace; a stray `)` is consumed rather
    /// than terminating the scan, mirroring CPython's post-parse
    /// "unbalanced parenthesis" check, which this scan leaves to the parser.
    fn alternation(&mut self, chars: &[char], mut i: usize, top: bool) -> Result<usize, Error> {
        loop {
            i = self.region(chars, i, top)?;
            if i >= chars.len() {
                return Ok(i);
            }
            match chars[i] {
                ')' => return Ok(i),
                '|' => {
                    i += 1;
                    self.mark_consumed(top);
                }
                _ => unreachable!("a region only stops at ')' or '|'"),
            }
        }
    }

    /// Scan one concatenation region up to an unmatched `)` or `|` that
    /// ends it, returning that offset.
    fn region(&mut self, chars: &[char], mut i: usize, top: bool) -> Result<usize, Error> {
        while i < chars.len() {
            match chars[i] {
                '\\' => {
                    i = (i + 2).min(chars.len());
                    self.mark_consumed(top);
                }
                '[' => {
                    i = skip_class(chars, i);
                    self.mark_consumed(top);
                }
                '(' => {
                    if chars.get(i + 1) == Some(&'?') {
                        match chars.get(i + 2).copied() {
                            Some('(') => {
                                i = self.conditional(chars, i)?;
                                self.mark_consumed(top);
                            }
                            Some('#') => {
                                let Some(close) = chars[i + 3..]
                                    .iter()
                                    .position(|&c| c == ')')
                                    .map(|at| i + 3 + at)
                                else {
                                    return Err(pre_error("missing ), unterminated comment", i));
                                };
                                i = close + 1;
                            }
                            Some('P') if chars.get(i + 3) == Some(&'<') => {
                                self.groups += 1;
                                let mut j = i + 4;
                                while j < chars.len() && chars[j] != '>' {
                                    j += 1;
                                }
                                let name: String =
                                    chars[i + 4..j.min(chars.len())].iter().collect();
                                self.names.push(name);
                                self.mark_consumed(top);
                                i = self.alternation(chars, j + 1, false)?;
                                if i >= chars.len() {
                                    return Ok(i);
                                }
                                i += 1;
                            }
                            _ => {
                                if let Some(end) = self.global_flag_group(chars, i) {
                                    if !top || self.consumed {
                                        return Err(pre_error(
                                            "global flags not at the start of the expression",
                                            i,
                                        ));
                                    }
                                    if chars[i + 2..end].contains(&'x') {
                                        self.verbose = true;
                                    }
                                    i = end + 1;
                                } else {
                                    // A scoped flag group `(?i:...)`, `(?:...)`,
                                    // lookaround, or an unknown extension; its
                                    // body is scanned as a nested alternation.
                                    self.mark_consumed(top);
                                    i = self.alternation(chars, i + 2, false)?;
                                    if i >= chars.len() {
                                        return Ok(i);
                                    }
                                    i += 1;
                                }
                            }
                        }
                    } else {
                        self.groups += 1;
                        self.mark_consumed(top);
                        i = self.alternation(chars, i + 1, false)?;
                        if i >= chars.len() {
                            return Ok(i);
                        }
                        i += 1;
                    }
                }
                ')' | '|' => return Ok(i),
                '#' if self.verbose => {
                    i = chars[i..]
                        .iter()
                        .position(|&c| c == '\n')
                        .map(|at| i + at + 1)
                        .unwrap_or(chars.len());
                }
                _ => {
                    if top && self.verbose && chars[i].is_whitespace() {
                        i += 1;
                    } else {
                        i += 1;
                        self.mark_consumed(top);
                    }
                }
            }
        }
        Ok(chars.len())
    }

    /// Validate one `(?(ref)yes|no)` group opening at `open`, returning the
    /// offset just past its closing `)`.
    fn conditional(&mut self, chars: &[char], open: usize) -> Result<usize, Error> {
        let name_start = open + 3;
        let mut close = name_start;
        while close < chars.len() && chars[close] != ')' {
            close += 1;
        }
        if close >= chars.len() {
            if name_start >= chars.len() {
                return Err(pre_error("missing group name", name_start));
            }
            return Err(pre_error("missing ), unterminated name", name_start));
        }
        let name: String = chars[name_start..close].iter().collect();
        if name.is_empty() {
            return Err(pre_error("missing group name", name_start));
        }
        if name.chars().all(|c| c.is_ascii_digit()) {
            match name.parse::<u64>() {
                Ok(0) => return Err(pre_error("bad group number", name_start)),
                Ok(value) if value >= MAX_GROUP_REFERENCE => {
                    return Err(Error::Unsupported {
                        what: format!("invalid group reference {value}"),
                        offset: name_start,
                    });
                }
                Ok(value) => self.digit_refs.push((value, name_start)),
                Err(_) => {
                    return Err(Error::Unsupported {
                        what: format!("invalid group reference {name}"),
                        offset: name_start,
                    });
                }
            }
        } else {
            if !is_python_identifier(&name) {
                return Err(Error::Unsupported {
                    what: format!(
                        "bad character in group name {}",
                        quote_name(&name, self.bytes)
                    ),
                    offset: name_start,
                });
            }
            if !self.names.contains(&name) {
                return Err(Error::Unsupported {
                    what: format!("unknown group name '{}'", name),
                    offset: name_start,
                });
            }
        }
        // A yes branch, then an optional no branch: single regions, so a
        // third top-level `|` stays visible to this check, as in CPython.
        let mut i = self.region(chars, close + 1, false)?;
        if i < chars.len() && chars[i] == '|' {
            i = self.region(chars, i + 1, false)?;
            if i < chars.len() && chars[i] == '|' {
                return Err(pre_error(
                    "conditional backref with more than two branches",
                    i,
                ));
            }
        }
        if i >= chars.len() || chars[i] != ')' {
            return Err(pre_error("missing ), unterminated subpattern", open));
        }
        Ok(i + 1)
    }

    /// The offset of the `)` closing a global inline flag group at `open`,
    /// or `None` when the group is scoped `(?i:...)` or an unknown extension.
    fn global_flag_group(&self, chars: &[char], open: usize) -> Option<usize> {
        let mut i = open + 2;
        while i < chars.len() && matches!(chars[i], 'i' | 'm' | 's' | 'x' | 'a' | 'L' | 'u' | '-') {
            i += 1;
        }
        if i > open + 2 && i < chars.len() && chars[i] == ')' {
            Some(i)
        } else {
            None
        }
    }

    fn mark_consumed(&mut self, top: bool) {
        if top {
            self.consumed = true;
        }
    }
}

/// Skip a bracketed class starting at `open`, returning the offset just past
/// its closing `]`. A `]` first (or after `^`) is a literal.
fn skip_class(chars: &[char], open: usize) -> usize {
    let mut i = (open + 1).min(chars.len());
    if i < chars.len() && chars[i] == '^' {
        i += 1;
    }
    if i < chars.len() && chars[i] == ']' {
        i += 1;
    }
    while i < chars.len() && chars[i] != ']' {
        if chars[i] == '\\' {
            i += 1;
        }
        i += 1;
    }
    (i + 1).min(chars.len())
}

/// A group name quoted as CPython reports it: verbatim for `str` patterns,
/// ascii-escaped for `bytes` patterns (`%a` of the latin-1 decoding).
fn quote_name(name: &str, bytes: bool) -> String {
    if !bytes {
        return format!("'{name}'");
    }
    let mut out = String::from("'");
    for c in name.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            out.push_str(&format!("\\x{:02x}", c as u32));
        }
    }
    out.push('\'');
    out
}

/// An [`Error::Unsupported`] with a static message.
fn pre_error(msg: &str, offset: usize) -> Error {
    Error::Unsupported {
        what: msg.to_string(),
        offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_of(pattern: &str) -> Error {
        match compile(pattern) {
            Ok(_) => panic!("pattern {pattern:?} compiled but should not"),
            Err(err) => err,
        }
    }

    #[test]
    fn end_anchor_normalization_preserves_escapes() {
        assert_eq!(normalize_end_anchor(r"\Z"), r"\z");
        assert_eq!(normalize_end_anchor(r"\\Z"), r"\\Z");
        assert_eq!(normalize_end_anchor(r"\\\Z"), r"\\\z");
        assert_eq!(normalize_end_anchor(r"a\Zb"), r"a\zb");
    }

    #[test]
    fn global_inline_flags_follow_cpython_placement_rules() {
        // Legal at the start, including runs and verbose whitespace.
        assert!(compile("(?i)a").is_ok());
        assert!(compile("(?i)(?s)a").is_ok());
        assert!(compile("(?x) (?i) a").is_ok());
        // A `(?#...)` comment does not consume the leading position. The
        // comment form itself is still rejected by the parser, so probe the
        // pre-scan directly.
        assert!(prevalidate("(?#c)(?i)a", false).is_ok());
        // A scoped flag group is legal anywhere, even empty.
        assert!(compile("a(?i:)").is_ok());
        assert!(compile("((?i:))").is_ok());
        // Misplaced global flags carry CPython's message and position.
        let err = error_of("(?:(?i)X)");
        assert_eq!(
            err.message(),
            "global flags not at the start of the expression"
        );
        assert_eq!(err.offset(), Some(3));
        let err = error_of("x(?i)");
        assert_eq!(err.offset(), Some(1));
        let err = error_of("(?i) X (?x)");
        assert_eq!(err.offset(), Some(7));
        let err = error_of("(^)?(?(1)(?i)X)");
        assert_eq!(
            err.message(),
            "global flags not at the start of the expression"
        );
        assert_eq!(err.offset(), Some(9));
    }

    #[test]
    fn conditional_groups_report_cpython_diagnostics() {
        let cases: &[(&str, &str, usize)] = &[
            (r"(?P<a>)(?(0)a|b)", "bad group number", 10),
            (r"()(?(-1)a|b)", "bad character in group name '-1'", 5),
            (r"()(?( 1 )a|b)", "bad character in group name ' 1 '", 5),
            (r"()(?(1", "missing ), unterminated name", 5),
            (r"()(?(1)a", "missing ), unterminated subpattern", 2),
            (
                r"()(?(1)a|b|c)",
                "conditional backref with more than two branches",
                10,
            ),
            (r"()(?(2)a)", "invalid group reference 2", 5),
            (r"(?(a))", "unknown group name 'a'", 3),
            (r"(?(", "missing group name", 3),
            (r"(?())", "missing group name", 3),
        ];
        for (pattern, message, offset) in cases {
            let err = error_of(pattern);
            assert_eq!(&err.message(), message, "pattern {pattern:?}");
            assert_eq!(err.offset(), Some(*offset), "pattern {pattern:?}");
        }
        // The `_sre.MAXGROUPS` bound, as `test_re_groupref_overflow` checks.
        let err = error_of("(?P<a>)(?(1073741823))");
        assert_eq!(err.message(), "invalid group reference 1073741823");
        assert_eq!(err.offset(), Some(10));
    }

    #[test]
    fn anchored_start_covers_only_start_anchored_programs() {
        assert!(compile(r"\Ay").unwrap().anchored_start);
        assert!(compile(r"^y").unwrap().anchored_start);
        assert!(compile(r"(?i)^y").unwrap().anchored_start);
        assert!(compile(r"(^y)").unwrap().anchored_start);
        assert!(compile(r"a?^y").unwrap().anchored_start);
        assert!(compile(r"y\A").unwrap().anchored_start); // never matches
        assert!(!compile(r"(?m)^y").unwrap().anchored_start);
        assert!(!compile(r"^a|b").unwrap().anchored_start);
        assert!(!compile(r"(?:^)?y").unwrap().anchored_start);
        assert!(!compile(r"(\Ay)*").unwrap().anchored_start);
        assert!(!compile(r"(?i)y").unwrap().anchored_start);
        assert!(!compile("").unwrap().anchored_start);
    }

    #[test]
    fn multiple_repeat_position_includes_one_modifier() {
        let cases: &[(&str, usize)] = &[
            (r"x**", 2),
            (r"x*?*", 3),
            (r"x**+", 2),
            (r"x*+*+", 3),
            (r"x{1,2}*+", 6),
            (r"x*+{1,2}+", 3),
            (r"x{1,2}+?", 7),
        ];
        for (pattern, offset) in cases {
            let err = error_of(pattern);
            assert_eq!(err.message(), "multiple repeat", "pattern {pattern:?}");
            assert_eq!(err.offset(), Some(*offset), "pattern {pattern:?}");
        }
    }

    #[test]
    fn first_set_prepares_scans_for_each_width() {
        let program = compile("@abc").unwrap();
        let first = program.first.as_ref().unwrap();
        assert!(matches!(first.scan::<u8>(), Some(Scan::One(0x40))));
        assert!(matches!(
            first.scan::<u16>(),
            Some(Scan::Bytes { target: 0x40, .. } | Scan::One(0x40))
        ));
        assert!(matches!(
            first.scan::<u32>(),
            Some(Scan::Bytes { target: 0x40, .. } | Scan::One(0x40))
        ));

        let program = compile("𐐷").unwrap();
        let first = program.first.as_ref().unwrap();
        assert!(first.scan::<u16>().is_none());
        assert!(matches!(
            first.scan::<u32>(),
            Some(
                Scan::Bytes {
                    target: 0x10437,
                    ..
                } | Scan::One(0x10437)
            )
        ));
    }
}
