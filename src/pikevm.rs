//! Linear-time PikeVM simulation over UCS code points.

use crate::Match;
use crate::compile::{AssertKind, Inst, Program};
use crate::dfa;
use crate::ucs::Unit;

/// Reusable PikeVM scratch buffers.
///
/// Create one with [`crate::Pattern::scratch`] and reuse it across searches to
/// avoid allocating thread lists and the epsilon-closure stack per call.
/// A `Scratch` is not shared between threads; give each worker its own.
pub struct Scratch {
    clist: ThreadList,
    nlist: ThreadList,
    stack: Vec<(usize, usize)>,
    dfa: Option<dfa::Cache>,
    dfa_disabled: bool,
    cap: CapLists,
    trail: Vec<(usize, usize)>,
}

impl Scratch {
    pub(crate) fn new(program_len: usize) -> Scratch {
        Scratch {
            clist: ThreadList::new(program_len),
            nlist: ThreadList::new(program_len),
            stack: Vec::new(),
            dfa: None,
            dfa_disabled: false,
            cap: CapLists::new(program_len),
            trail: Vec::new(),
        }
    }
}

/// Find a match, preferring the lazy DFA and falling back to the PikeVM.
pub(crate) fn find<U: Unit>(
    prog: &Program,
    scratch: &mut Scratch,
    hay: &[U],
    search_start: usize,
    retry_nonempty_at: Option<usize>,
) -> Option<Match> {
    if prog.first.is_some() && !scratch.dfa_disabled {
        if scratch.dfa.is_none() {
            match dfa::Cache::new(prog) {
                Some(cache) => scratch.dfa = Some(cache),
                None => scratch.dfa_disabled = true,
            }
        }
        let result = scratch
            .dfa
            .as_mut()
            .map(|cache| dfa::find_candidates(prog, cache, hay, search_start, retry_nonempty_at));
        if let Some(result) = result {
            match result {
                dfa::FindResult::Match(found) => return Some(found),
                dfa::FindResult::NoMatch => return None,
                dfa::FindResult::Overflow => scratch.dfa_disabled = true,
            }
        }
    }
    find_pike(prog, scratch, hay, search_start, retry_nonempty_at)
}

/// Find the leftmost-first match in `hay` starting at `search_start`.
///
/// When `retry_nonempty_at` is `Some(p)`, an empty match exactly at `p` is
/// ignored, implementing Python 3.7+ empty-match iteration.
fn find_pike<U: Unit>(
    prog: &Program,
    scratch: &mut Scratch,
    hay: &[U],
    search_start: usize,
    retry_nonempty_at: Option<usize>,
) -> Option<Match> {
    let Scratch {
        clist,
        nlist,
        stack,
        dfa: _,
        dfa_disabled: _,
        cap: _,
        trail: _,
    } = scratch;
    clist.clear();
    nlist.clear();

    if search_start > hay.len() {
        return None;
    }

    let first = prog.first.as_ref().and_then(|first| first.scan::<U>());
    let mut matched: Option<Match> = None;
    let mut pos = search_start;

    while pos <= hay.len() {
        nlist.clear();

        if clist.is_empty() && matched.is_none() {
            if let Some(first) = &first {
                match first.find_next(hay, pos) {
                    Some(next) => pos = next,
                    None => break,
                }
            }
        }

        if matched.is_none() {
            add_thread(clist, stack, prog, prog.start, pos, pos, hay);
        }

        let mut index = 0;
        while index < clist.len() {
            let pc = clist.pcs[index];
            let start = clist.starts[index];
            match prog.insts[pc] {
                Inst::Char(expected) => {
                    if pos < hay.len() && hay[pos].into() == expected {
                        add_thread(nlist, stack, prog, pc + 1, start, pos + 1, hay);
                    }
                }
                Inst::Class(class_id) => {
                    if pos < hay.len() && prog.classes[class_id].contains(hay[pos].into()) {
                        add_thread(nlist, stack, prog, pc + 1, start, pos + 1, hay);
                    }
                }
                Inst::Match => {
                    let empty_at_retry = retry_nonempty_at == Some(pos) && start == pos;
                    if !empty_at_retry {
                        matched = Some(Match::new(start, pos));
                        break;
                    }
                }
                Inst::Split(_, _) | Inst::Jump(_) | Inst::Assert(_) | Inst::Save(_) => {
                    unreachable!("epsilon instruction in a thread list")
                }
            }
            index += 1;
        }

        if matched.is_some() && nlist.is_empty() {
            break;
        }
        std::mem::swap(clist, nlist);
        pos += 1;
    }

    matched
}

fn add_thread<U: Unit>(
    list: &mut ThreadList,
    stack: &mut Vec<(usize, usize)>,
    prog: &Program,
    pc: usize,
    thread_start: usize,
    pos: usize,
    hay: &[U],
) {
    stack.clear();
    stack.push((pc, thread_start));
    while let Some((pc, start)) = stack.pop() {
        if !list.mark(pc) {
            continue;
        }
        match prog.insts[pc] {
            Inst::Split(first, second) => {
                stack.push((second, start));
                stack.push((first, start));
            }
            Inst::Jump(target) => stack.push((target, start)),
            Inst::Assert(kind) => {
                if assert_holds(kind, hay, pos, prog.ascii) {
                    stack.push((pc + 1, start));
                }
            }
            Inst::Char(_) | Inst::Class(_) | Inst::Match => list.push(pc, start),
            Inst::Save(_) => {
                // The group-0 engine does not track slots.
                if pc + 1 < prog.insts.len() {
                    stack.push((pc + 1, start));
                }
            }
        }
    }
}

/// Whether `kind` holds at `pos`, which needs the units on both sides.
fn assert_holds<U: Unit>(kind: AssertKind, hay: &[U], pos: usize, ascii: bool) -> bool {
    let prev = if pos == 0 {
        None
    } else {
        Some(hay[pos - 1].into())
    };
    let next = hay.get(pos).map(|unit| (*unit).into());
    kind.holds(prev, next, pos, hay.len(), ascii)
}

/// Thread lists with per-thread capture slots.
///
/// Used only by [`capture_slots`], which runs over a single known match span, so
/// nothing here is on the group-0 hot path. Slots are stored flat: thread `i`
/// owns `slots[i * width..(i + 1) * width]`.
pub(crate) struct CapLists {
    pcs: [Vec<usize>; 2],
    slots: [Vec<usize>; 2],
    seen: [Vec<u32>; 2],
    generation: [u32; 2],
    arena: Vec<usize>,
    stack: Vec<(usize, usize)>,
    width: usize,
    matched: Vec<usize>,
}

impl CapLists {
    fn new(program_len: usize) -> CapLists {
        CapLists {
            pcs: [Vec::new(), Vec::new()],
            slots: [Vec::new(), Vec::new()],
            seen: [vec![0; program_len], vec![0; program_len]],
            generation: [1, 1],
            arena: Vec::new(),
            stack: Vec::new(),
            width: 0,
            matched: Vec::new(),
        }
    }

    fn set_width(&mut self, width: usize) {
        self.width = width;
        if self.matched.len() < width {
            self.matched.resize(width, usize::MAX);
        }
    }

    fn clear(&mut self, list: usize) {
        self.pcs[list].clear();
        self.slots[list].clear();
        self.generation[list] = self.generation[list].wrapping_add(1);
        if self.generation[list] == 0 {
            self.seen[list].fill(0);
            self.generation[list] = 1;
        }
    }

    fn mark(&mut self, list: usize, pc: usize) -> bool {
        if self.seen[list][pc] == self.generation[list] {
            false
        } else {
            self.seen[list][pc] = self.generation[list];
            true
        }
    }

    fn push_thread(&mut self, list: usize, pc: usize, arena_offset: usize) {
        self.pcs[list].push(pc);
        self.slots[list].extend_from_slice(&self.arena[arena_offset..arena_offset + self.width]);
    }
}

/// Extract capture slots for a match that is already known to span
/// `start..end`.
///
/// The pass is anchored at `start` and accepts a `Match` only at `end`, which is
/// exactly the answer the DFA produced, so it re-derives the group spans without
/// re-deciding the match. `slots` holds `2 * (groups + 1)` entries; entry 0 and
/// 1 receive the whole-match span.
/// Extract capture slots for a match already known to span `start..end`.
///
/// The DFA finds the span on the hot path. Recovering group spans with the
/// PikeVM is O(span * program), which dominates for patterns with unrolled
/// repetitions, so a bounded backtracker over the same span is tried first and
/// the PikeVM is the fallback when the backtracker exceeds its budget.
pub(crate) fn capture_slots<U: Unit>(
    prog: &Program,
    scratch: &mut Scratch,
    hay: &[U],
    start: usize,
    end: usize,
    slots: &mut [usize],
) -> bool {
    let width = 2 * (prog.groups + 1);
    if slots.len() < width || start > end || end > hay.len() {
        return false;
    }
    slots.fill(usize::MAX);
    slots[0] = start;
    slots[1] = end;
    let mut trail = std::mem::take(&mut scratch.trail);
    let matched = backtrack_slots(prog, hay, start, end, slots, &mut trail);
    trail.clear();
    scratch.trail = trail;
    if matched {
        return true;
    }
    capture_slots_pike(prog, scratch, hay, start, end, slots)
}

/// Bounded backtracking capture extraction over a known span.
///
/// Backtracking is only attempted for spans up to this many code units.
///
/// The backtracker is recursive, so its stack use grows with the span. A test
/// with a 2,000-unit span overflowed the default stack in a debug build, which
/// makes a hard limit mandatory rather than an optimisation.
const MAX_BACKTRACK_SPAN: usize = 1_024;

/// Maximum recursion depth of the backtracker.
///
/// Bounds stack use independently of the span, because a deeply nested program
/// can recurse without consuming input.
const MAX_BACKTRACK_DEPTH: usize = 1_024;

/// Absolute ceiling on backtracking steps for one match.
///
/// The span-scaled budget alone is not a bound: at the maximum span with a
/// 200-instruction program it reaches about 2.6 billion steps, which is
/// seconds of work for a single match. This cap keeps the worst case per match
/// in the low milliseconds, after which extraction falls back to the PikeVM.
const MAX_BACKTRACK_STEPS: usize = 1_000_000;

/// Steps allowed for a span of `span` units in a program of `insts`
/// instructions.
fn backtrack_budget(span: usize, insts: usize) -> usize {
    const MIN_BUDGET: usize = 100_000;
    MIN_BUDGET
        .max(200 * (span + 2) * insts)
        .min(MAX_BACKTRACK_STEPS)
}

/// Returns `false` when the step budget is exhausted or the span is too long,
/// which makes [`capture_slots`] fall back to the PikeVM. A failed search never
/// reports a wrong span: only a successful walk of the exact span returns.
fn backtrack_slots<U: Unit>(
    prog: &Program,
    hay: &[U],
    start: usize,
    end: usize,
    slots: &mut [usize],
    trail: &mut Vec<(usize, usize)>,
) -> bool {
    let span = end - start;
    if span > MAX_BACKTRACK_SPAN {
        return false;
    }
    let budget = backtrack_budget(span, prog.insts.len());
    let mut bt = Backtrack {
        prog,
        hay,
        end,
        slots,
        trail,
        steps: 0,
        budget,
        aborted: false,
    };
    let matched = bt.step(prog.start, start, 0);
    matched && !bt.aborted
}

struct Backtrack<'a, U: Unit> {
    prog: &'a Program,
    hay: &'a [U],
    end: usize,
    slots: &'a mut [usize],
    trail: &'a mut Vec<(usize, usize)>,
    steps: usize,
    budget: usize,
    aborted: bool,
}

impl<U: Unit> Backtrack<'_, U> {
    fn undo_to(&mut self, mark: usize) {
        while self.trail.len() > mark {
            let (slot, old) = self.trail.pop().expect("trail length checked");
            self.slots[slot] = old;
        }
    }

    fn step(&mut self, pc: usize, pos: usize, depth: usize) -> bool {
        if self.aborted {
            return false;
        }
        if self.steps >= self.budget || depth >= MAX_BACKTRACK_DEPTH {
            self.aborted = true;
            return false;
        }
        self.steps += 1;
        match self.prog.insts[pc] {
            Inst::Char(expected) => {
                pos < self.end
                    && self.hay[pos].into() == expected
                    && self.step(pc + 1, pos + 1, depth + 1)
            }
            Inst::Class(class_id) => {
                pos < self.end
                    && self.prog.classes[class_id].contains(self.hay[pos].into())
                    && self.step(pc + 1, pos + 1, depth + 1)
            }
            Inst::Split(first, second) => {
                let mark = self.trail.len();
                if self.step(first, pos, depth + 1) {
                    return true;
                }
                if self.aborted {
                    return false;
                }
                self.undo_to(mark);
                self.step(second, pos, depth + 1)
            }
            Inst::Jump(target) => self.step(target, pos, depth + 1),
            Inst::Save(slot) => {
                let old = self.slots[slot];
                self.slots[slot] = pos;
                self.trail.push((slot, old));
                if self.step(pc + 1, pos, depth + 1) {
                    true
                } else {
                    self.undo_to(self.trail.len() - 1);
                    false
                }
            }
            Inst::Assert(kind) => {
                assert_holds(kind, self.hay, pos, self.prog.ascii)
                    && self.step(pc + 1, pos, depth + 1)
            }
            Inst::Match => pos == self.end,
        }
    }
}

fn capture_slots_pike<U: Unit>(
    prog: &Program,
    scratch: &mut Scratch,
    hay: &[U],
    start: usize,
    end: usize,
    slots: &mut [usize],
) -> bool {
    let width = 2 * (prog.groups + 1);
    if slots.len() < width || start > end || end > hay.len() {
        return false;
    }
    slots.fill(usize::MAX);
    slots[0] = start;
    slots[1] = end;

    let cap = &mut scratch.cap;
    cap.set_width(width);
    cap.clear(0);
    cap.clear(1);

    cap.arena.clear();
    cap.arena.extend_from_slice(slots);
    cap.stack.clear();
    cap.stack.push((prog.start, 0));
    close_with_slots(prog, cap, 0, start, hay);

    let mut list = 0usize;
    let mut found: Option<usize> = None;
    let mut pos = start;
    loop {
        if cap.pcs[list].is_empty() {
            break;
        }
        let next = 1 - list;
        cap.clear(next);
        cap.arena.clear();
        let mut stop = false;
        let mut index = 0;
        while index < cap.pcs[list].len() {
            let pc = cap.pcs[list][index];
            let offset = index * width;
            match prog.insts[pc] {
                Inst::Char(expected) => {
                    if pos < end && hay[pos].into() == expected {
                        let arena_offset = cap.arena.len();
                        cap.arena
                            .extend_from_slice(&cap.slots[list][offset..offset + width]);
                        cap.stack.clear();
                        cap.stack.push((pc + 1, arena_offset));
                        close_with_slots(prog, cap, next, pos + 1, hay);
                    }
                }
                Inst::Class(class_id) => {
                    if pos < end && prog.classes[class_id].contains(hay[pos].into()) {
                        let arena_offset = cap.arena.len();
                        cap.arena
                            .extend_from_slice(&cap.slots[list][offset..offset + width]);
                        cap.stack.clear();
                        cap.stack.push((pc + 1, arena_offset));
                        close_with_slots(prog, cap, next, pos + 1, hay);
                    }
                }
                Inst::Match => {
                    if pos == end {
                        found = Some(index);
                        stop = true;
                        break;
                    }
                }
                Inst::Split(_, _) | Inst::Jump(_) | Inst::Assert(_) | Inst::Save(_) => {
                    unreachable!("epsilon instruction in a thread list")
                }
            }
            index += 1;
        }
        if let Some(index) = found {
            let offset = index * width;
            cap.matched[..width].copy_from_slice(&cap.slots[list][offset..offset + width]);
        }
        if stop || pos == end {
            break;
        }
        let [pcs_a, pcs_b] = &mut cap.pcs;
        std::mem::swap(pcs_a, pcs_b);
        let [slots_a, slots_b] = &mut cap.slots;
        std::mem::swap(slots_a, slots_b);
        let [seen_a, seen_b] = &mut cap.seen;
        std::mem::swap(seen_a, seen_b);
        let [gen_a, gen_b] = &mut cap.generation;
        std::mem::swap(gen_a, gen_b);
        list = 0;
        pos += 1;
    }

    match found {
        Some(_) => {
            slots.copy_from_slice(&cap.matched[..width]);
            true
        }
        None => false,
    }
}

/// Epsilon closure that writes `Save` instructions into the thread's slots.
fn close_with_slots<U: Unit>(
    prog: &Program,
    cap: &mut CapLists,
    list: usize,
    pos: usize,
    hay: &[U],
) {
    let width = cap.width;
    while let Some((pc, offset)) = cap.stack.pop() {
        if !cap.mark(list, pc) {
            continue;
        }
        match prog.insts[pc] {
            Inst::Split(first, second) => {
                let second_offset = cap.arena.len();
                cap.arena.reserve(width);
                for index in 0..width {
                    let value = cap.arena[offset + index];
                    cap.arena.push(value);
                }
                cap.stack.push((second, second_offset));
                cap.stack.push((first, offset));
            }
            Inst::Jump(target) => cap.stack.push((target, offset)),
            Inst::Save(slot) => {
                cap.arena[offset + slot] = pos;
                if pc + 1 < prog.insts.len() {
                    cap.stack.push((pc + 1, offset));
                }
            }
            Inst::Assert(kind) => {
                if assert_holds(kind, hay, pos, prog.ascii) && pc + 1 < prog.insts.len() {
                    cap.stack.push((pc + 1, offset));
                }
            }
            Inst::Char(_) | Inst::Class(_) | Inst::Match => {
                cap.push_thread(list, pc, offset);
            }
        }
    }
}

/// A priority-ordered thread list with a sparse visited set.
struct ThreadList {
    pcs: Vec<usize>,
    starts: Vec<usize>,
    seen: Vec<u32>,
    generation: u32,
}

impl ThreadList {
    fn new(program_len: usize) -> ThreadList {
        ThreadList {
            pcs: Vec::with_capacity(program_len.min(1024)),
            starts: Vec::with_capacity(program_len.min(1024)),
            seen: vec![0; program_len],
            generation: 1,
        }
    }

    fn clear(&mut self) {
        self.pcs.clear();
        self.starts.clear();
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
    }

    fn mark(&mut self, pc: usize) -> bool {
        if self.seen[pc] == self.generation {
            false
        } else {
            self.seen[pc] = self.generation;
            true
        }
    }

    fn push(&mut self, pc: usize, start: usize) {
        self.pcs.push(pc);
        self.starts.push(start);
    }

    fn len(&self) -> usize {
        self.pcs.len()
    }

    fn is_empty(&self) -> bool {
        self.pcs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;

    fn compare(pattern: &str, hay: &str) {
        let program = compile(pattern).unwrap();
        let units: Vec<u8> = hay.bytes().collect();
        let mut dfa_scratch = Scratch::new(program.insts.len());
        let mut pike_scratch = Scratch::new(program.insts.len());
        let dfa = find(&program, &mut dfa_scratch, &units, 0, None).map(|m| (m.start(), m.end()));
        let pike =
            find_pike(&program, &mut pike_scratch, &units, 0, None).map(|m| (m.start(), m.end()));
        assert_eq!(dfa, pike, "pattern {pattern:?} haystack {hay:?}");
    }

    #[test]
    fn dfa_matches_pikevm_on_supported_patterns() {
        let patterns = [
            "abc",
            "a|ab",
            "ab|a",
            "a*",
            "a+b",
            "a?b",
            "(a|)b",
            "(a|b)*c",
            "^abc",
            r"\Aabc",
            r"abc\Z",
            r"a\Z|b",
            r"\Za|a",
            "[a-c]+x",
            "[^a]+",
            ".",
            "a{2,3}",
            "(a*)*b",
            "a*b",
            "a|b|c",
            r"(a|ab)\Z",
            r"(ab|a)\Z",
            r"a\Z|ab",
            r"^b|a",
            r"\A(b|a)",
        ];
        let hays = [
            "", "a", "b", "ab", "abc", "xabc", "abcx", "aaab", "baa", "zzb", "\n", "ab\n",
        ];
        for pattern in patterns {
            for hay in hays {
                compare(pattern, hay);
            }
        }
    }

    #[test]
    fn dfa_matches_pikevm_on_ucs2_and_ucs4() {
        let program = compile("β+").unwrap();
        let two: Vec<u16> = "aββc".chars().map(|c| c as u16).collect();
        let mut dfa_scratch = Scratch::new(program.insts.len());
        let mut pike_scratch = Scratch::new(program.insts.len());
        assert_eq!(
            find(&program, &mut dfa_scratch, &two, 0, None).map(|m| (m.start(), m.end())),
            find_pike(&program, &mut pike_scratch, &two, 0, None).map(|m| (m.start(), m.end()))
        );

        let program = compile("𐐷+").unwrap();
        let four: Vec<u32> = "a𐐷𐐷b".chars().map(|c| c as u32).collect();
        let mut dfa_scratch = Scratch::new(program.insts.len());
        let mut pike_scratch = Scratch::new(program.insts.len());
        assert_eq!(
            find(&program, &mut dfa_scratch, &four, 0, None).map(|m| (m.start(), m.end())),
            find_pike(&program, &mut pike_scratch, &four, 0, None).map(|m| (m.start(), m.end()))
        );
    }

    /// Whole-match span plus one entry per capture group, `(MAX, MAX)` when
    /// the group did not participate.
    type Spans = (Vec<usize>, Vec<(usize, usize)>);

    fn spans_for<U: Unit>(pattern: &str, hay: &[U]) -> Option<Spans> {
        let program = compile(pattern).unwrap();
        let width = 2 * (program.groups + 1);
        let mut scratch = Scratch::new(program.insts.len());
        let found = find(&program, &mut scratch, hay, 0, None)?;
        let mut slots = vec![usize::MAX; width];
        assert!(
            capture_slots(
                &program,
                &mut scratch,
                hay,
                found.start(),
                found.end(),
                &mut slots
            ),
            "capture pass failed for {pattern:?}"
        );
        let span = (slots[0], slots[1]);
        let groups = (1..=program.groups)
            .map(|index| (slots[2 * index], slots[2 * index + 1]))
            .collect();
        Some((vec![span.0, span.1], groups))
    }

    /// Spans produced by CPython `re` for the same patterns and haystacks.
    #[test]
    fn capture_spans_match_cpython_re() {
        #[rustfmt::skip]
        type Case = (&'static str, &'static str, &'static [(i64, i64)]);
        let cases: &[Case] = &[
            (r"(a)(b)", "ab", &[(0, 2), (0, 1), (1, 2)]),
            (r"(a+)(b+)", "aaabbb", &[(0, 6), (0, 3), (3, 6)]),
            (r"((a)(b))", "ab", &[(0, 2), (0, 2), (0, 1), (1, 2)]),
            (r"(a)+", "aaa", &[(0, 3), (2, 3)]),
            (r"(a)|(b)", "b", &[(0, 1), (-1, -1), (0, 1)]),
            (r"(a*)(a*)", "aa", &[(0, 2), (0, 2), (2, 2)]),
            (r"(a|ab)(b?)", "ab", &[(0, 2), (0, 1), (1, 2)]),
            (r"(ab|a)(b)", "ab", &[(0, 2), (0, 1), (1, 2)]),
            (r"()(a)", "a", &[(0, 1), (0, 0), (0, 1)]),
            (r"(a)?(b)", "b", &[(0, 1), (-1, -1), (0, 1)]),
            (r"(a{2,3})(a)", "aaaa", &[(0, 4), (0, 3), (3, 4)]),
            (r"(?:(a)|(b))+", "ab", &[(0, 2), (0, 1), (1, 2)]),
            (r"((a)*)", "aaa", &[(0, 3), (0, 3), (2, 3)]),
            (
                r"([a-z]+)([0-9]+)",
                "abc123def456",
                &[(0, 6), (0, 3), (3, 6)],
            ),
        ];
        for (pattern, hay, expected) in cases {
            let units: Vec<u8> = hay.bytes().collect();
            let (span, groups) = spans_for(pattern, &units).expect("no match");
            assert_eq!(
                &span[..],
                &[expected[0].0 as usize, expected[0].1 as usize],
                "span {pattern:?}"
            );
            for (index, &(start, end)) in expected[1..].iter().enumerate() {
                let got = groups[index];
                if start < 0 {
                    assert_eq!(
                        got,
                        (usize::MAX, usize::MAX),
                        "group {} of {pattern:?}",
                        index + 1
                    );
                } else {
                    assert_eq!(
                        got,
                        (start as usize, end as usize),
                        "group {} of {pattern:?}",
                        index + 1
                    );
                }
            }
        }
    }

    #[test]
    fn capture_spans_work_on_ucs2_and_ucs4() {
        let units2: Vec<u16> = "aab".encode_utf16().collect();
        let (span, groups) = spans_for(r"(a+)(b)", &units2).unwrap();
        assert_eq!(span, vec![0, 3]);
        assert_eq!(groups, vec![(0, 2), (2, 3)]);

        let units4: Vec<u32> = "aab".chars().map(u32::from).collect();
        let (span, groups) = spans_for(r"(a+)(b)", &units4).unwrap();
        assert_eq!(span, vec![0, 3]);
        assert_eq!(groups, vec![(0, 2), (2, 3)]);

        let wide: Vec<u32> = "\u{1F600}\u{1F600}x".chars().map(u32::from).collect();
        let (span, groups) = spans_for(r"(.+)(x)", &wide).unwrap();
        assert_eq!(span, vec![0, 3]);
        assert_eq!(groups, vec![(0, 2), (2, 3)]);
    }

    #[test]
    fn budget_is_capped() {
        assert_eq!(backtrack_budget(0, 10), 100_000);
        assert_eq!(backtrack_budget(32, 200), 1_000_000);
        // The span-scaled term alone would be far larger than the cap.
        assert_eq!(backtrack_budget(65_536, 200), MAX_BACKTRACK_STEPS);
        assert_eq!(backtrack_budget(1_024, 200), 1_000_000);
        assert_eq!(backtrack_budget(1_024, 1), 205_200);
    }

    /// A pattern that forces the backtracker to abort must still return correct
    /// group spans through the PikeVM fallback.
    #[test]
    fn pathological_pattern_falls_back_and_stays_correct() {
        let program = compile(r"(a|a)*b").unwrap();
        let hay: Vec<u8> = std::iter::repeat_n(b'a', 2_000)
            .chain(std::iter::once(b'b'))
            .collect();
        let mut scratch = Scratch::new(program.insts.len());
        let found = find(&program, &mut scratch, &hay, 0, None).unwrap();
        assert_eq!((found.start(), found.end()), (0, 2_001));
        let mut slots = vec![usize::MAX; 2 * (program.groups + 1)];
        assert!(
            capture_slots(
                &program,
                &mut scratch,
                &hay,
                found.start(),
                found.end(),
                &mut slots
            ),
            "fallback produced no slots"
        );
        assert_eq!((slots[0], slots[1]), (0, 2_001));
        // Python: re.search(r"(a|a)*b", "a" * 2000 + "b").span(1) == (1999, 2000)
        assert_eq!((slots[2], slots[3]), (1_999, 2_000));
    }

    #[test]
    #[ignore = "manual timing probe; run with --release --ignored --nocapture"]
    fn capture_costs() {
        fn time<F: FnMut()>(iterations: usize, mut run: F) -> std::time::Duration {
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                run();
            }
            start.elapsed() / iterations as u32
        }

        let cases: &[(usize, &str)] = &[
            (31, r"@([a-zA-Z])([a-zA-Z0-9_]{4,31})"),
            (1_024, r"(a|a)*b"),
            (65_536, r"(a|a)*b"),
            (1_000_000, r"(a|a)*b"),
        ];
        for &(span, pattern) in cases {
            let program = compile(pattern).unwrap();
            let width = 2 * (program.groups + 1);
            let hay: Vec<u8> = if pattern.contains("(?") || pattern.starts_with('@') {
                let mut text = String::from("@a").into_bytes();
                text.resize(span, b'x');
                text
            } else {
                let mut text = vec![b'a'; span - 1];
                text.push(b'b');
                text
            };
            let mut scratch = Scratch::new(program.insts.len());
            let hay2 = &hay[..span.min(hay.len())];
            let found = find(&program, &mut scratch, hay2, 0, None);
            let mut slots = vec![usize::MAX; width];
            let mut trail = Vec::new();
            let (start, end) = match found {
                Some(m) => (m.start(), m.end()),
                None => continue,
            };
            let bt = time(200, || {
                let mut s = vec![usize::MAX; width];
                backtrack_slots(&program, hay2, start, end, &mut s, &mut trail);
            });
            let pike = time(20, || {
                capture_slots_pike(&program, &mut scratch, hay2, start, end, &mut slots);
            });
            println!("span={span:>7} pattern={pattern:35} backtrack={bt:>10?} pike={pike:>10?}");
        }
    }

    #[test]
    fn capture_pass_is_anchored_at_the_match() {
        let program = compile(r"(a+)(b)").unwrap();
        let hay: Vec<u8> = b"xxaaab".to_vec();
        let mut scratch = Scratch::new(program.insts.len());
        let found = find(&program, &mut scratch, &hay, 0, None).unwrap();
        assert_eq!((found.start(), found.end()), (2, 6));
        let mut slots = vec![usize::MAX; 2 * (program.groups + 1)];
        assert!(capture_slots(
            &program,
            &mut scratch,
            &hay,
            found.start(),
            found.end(),
            &mut slots
        ));
        assert_eq!(slots, vec![2, 6, 2, 5, 5, 6]);
    }
}
