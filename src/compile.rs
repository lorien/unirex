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
    ) -> bool {
        match self {
            AssertKind::Start => pos == 0,
            AssertKind::End => pos == len,
            AssertKind::EndLine => pos == len || (pos + 1 == len && next == Some('\n' as u32)),
            AssertKind::StartLine => pos == 0 || prev == Some('\n' as u32),
            AssertKind::EndAnyLine => pos == len || next == Some('\n' as u32),
            AssertKind::WordBoundary => {
                unicode_tables::is_word(prev.unwrap_or(0))
                    != unicode_tables::is_word(next.unwrap_or(0))
            }
            AssertKind::NotWordBoundary => {
                unicode_tables::is_word(prev.unwrap_or(0))
                    == unicode_tables::is_word(next.unwrap_or(0))
            }
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
    if pattern.len() > MAX_PATTERN_LEN {
        return Err(Error::TooLarge {
            what: "pattern length",
            offset: 0,
        });
    }

    let normalized = normalize_end_anchor(pattern);
    validate_python_escapes(&normalized)?;
    let mut parser = ast::parse::ParserBuilder::new().octal(true).build();
    let ast = parser.parse(&normalized)?;
    let mut compiler = Compiler::new();
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
}

struct Compiler {
    flags: Flags,
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
    fn new() -> Compiler {
        Compiler {
            flags: Flags::default(),
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
        Ok(Program {
            insts: self.insts,
            classes: self.classes,
            start: 0,
            first,
            groups: self.groups,
            uses_context: self.uses_context,
            names: self.names,
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
                        ast::Flag::IgnoreWhitespace => {
                            if value {
                                return Err(unsupported("the x flag", &item.span));
                            }
                        }
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
                if !self.insts.is_empty() {
                    return Err(unsupported(
                        "global flags outside the start of the pattern",
                        &flags.span,
                    ));
                }
                self.apply_flags(&flags.flags.items, &flags.span)?;
                Ok(())
            }
            Ast::Literal(lit) => {
                check_literal(lit)?;
                let code_point = lit.c as u32;
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
                let ranges = match class.kind {
                    ast::ClassPerlKind::Word => unicode_tables::WORD.to_vec(),
                    ast::ClassPerlKind::Digit => unicode_tables::DIGIT.to_vec(),
                    ast::ClassPerlKind::Space => unicode_tables::SPACE.to_vec(),
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
                let table = match class.kind {
                    ast::ClassPerlKind::Word => unicode_tables::WORD,
                    ast::ClassPerlKind::Digit => unicode_tables::DIGIT,
                    ast::ClassPerlKind::Space => unicode_tables::SPACE,
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

fn unsupported(what: &'static str, span: &Span) -> Error {
    Error::Unsupported {
        what,
        offset: span.start.offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_anchor_normalization_preserves_escapes() {
        assert_eq!(normalize_end_anchor(r"\Z"), r"\z");
        assert_eq!(normalize_end_anchor(r"\\Z"), r"\\Z");
        assert_eq!(normalize_end_anchor(r"\\\Z"), r"\\\z");
        assert_eq!(normalize_end_anchor(r"a\Zb"), r"a\zb");
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
