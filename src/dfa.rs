//! Lazy DFA over code-point equivalence classes.
//!
//! The DFA is built on demand from the NFA and cached in a [`Scratch`]. It
//! preserves leftmost-first priority by pruning states after the first `Match`
//! at each position. Start and end assertions are resolved from position
//! context; end assertions are kept pending until the final position.

use std::collections::HashMap;

use crate::Match;
use crate::compile::{AssertKind, Inst, Program};
use crate::ucs::{UcsKind, Unit};

const MAX_CODEPOINT: u32 = 0x10FFFF;
const MAX_DFA_STATES: usize = 4096;
const MAX_TRANSITION_CELLS: usize = 4_000_000;
const UNKNOWN: u32 = u32::MAX;
const DEAD: u32 = u32::MAX - 1;

/// The result of a DFA search.
pub(crate) enum FindResult {
    /// A match was found.
    Match(Match),
    /// No match was found.
    NoMatch,
    /// The state cache overflowed or the DFA cannot represent the input.
    /// The caller must fall back to the PikeVM.
    Overflow,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct StateData {
    /// Consuming instructions in priority order.
    consume: Vec<usize>,
    /// `Assert(End)` instructions kept until the final position, in priority
    /// order.
    pending_end: Vec<usize>,
    /// A `Match` was reached at the current position; lower-priority states
    /// were pruned.
    matched: bool,
}

/// A lazy DFA cache.
///
/// One cache belongs to one compiled pattern and one search worker. It is
/// stored in `Scratch`, not in `Regex`, so searches on different threads do
/// not contend.
pub(crate) struct Cache {
    ucs1: Vec<u16>,
    ucs2: Vec<u16>,
    ucs4: Vec<u16>,
    representatives: Vec<u32>,
    num_classes: usize,
    states: Vec<StateData>,
    matched: Vec<bool>,
    index: HashMap<StateData, usize>,
    transitions: Vec<u32>,
    initial: [Option<usize>; 4],
    overflowed: bool,
    limit: usize,
}

impl Cache {
    /// Build the class maps and an empty state cache.
    pub(crate) fn new(program: &Program) -> Option<Cache> {
        Cache::new_with_limit(program, MAX_DFA_STATES)
    }

    fn new_with_limit(program: &Program, limit: usize) -> Option<Cache> {
        if program.uses_context {
            // Word boundaries and `$` depend on the unit before or after the
            // current position, which a DFA state cannot see. Such patterns
            // stay on the PikeVM; see ADR-0017 and `architecture.md`.
            return None;
        }
        let boundaries = class_boundaries(program);
        if boundaries.len() < 2 {
            return None;
        }
        let class_count = boundaries.len() - 1;
        if class_count > usize::from(u16::MAX) {
            return None;
        }
        let representatives = boundaries[..class_count].to_vec();
        let mut cache = Cache {
            ucs1: build_map(&boundaries, 0x100),
            ucs2: build_map(&boundaries, 0x1_0000),
            ucs4: build_map(&boundaries, 0x11_0000),
            representatives,
            num_classes: class_count,
            states: Vec::new(),
            matched: Vec::new(),
            index: HashMap::new(),
            transitions: Vec::new(),
            initial: [None; 4],
            overflowed: false,
            limit,
        };
        for (index, (at_start, at_end)) in
            [(false, false), (true, false), (false, true), (true, true)]
                .into_iter()
                .enumerate()
        {
            let state = closure(program, &[program.start], at_start, at_end);
            cache.initial[index] = cache.intern(state);
        }
        Some(cache)
    }

    #[inline]
    fn class_id(&self, kind: UcsKind, unit: u32) -> Option<u16> {
        let index = usize::try_from(unit).ok()?;
        match kind {
            UcsKind::Ucs1 => self.ucs1.get(index).copied(),
            UcsKind::Ucs2 => self.ucs2.get(index).copied(),
            UcsKind::Ucs4 => self.ucs4.get(index).copied(),
        }
    }

    fn initial_state(&self, at_start: bool, at_end: bool) -> Option<usize> {
        let index = usize::from(at_start) | (usize::from(at_end) << 1);
        self.initial[index]
    }

    #[inline]
    fn transition(&mut self, program: &Program, state_id: usize, class_id: u16) -> Option<usize> {
        let class = usize::from(class_id);
        let cell = state_id * self.num_classes + class;
        let cached = self.transitions[cell];
        if cached != UNKNOWN {
            return if cached == DEAD {
                None
            } else {
                Some(cached as usize)
            };
        }
        let representative = self.representatives[class];
        let mut seeds = Vec::new();
        {
            let consume = &self.states[state_id].consume;
            for &pc in consume {
                if instruction_matches(program, pc, representative) {
                    seeds.push(pc + 1);
                }
            }
        }
        let next = if seeds.is_empty() {
            None
        } else {
            let state = closure(program, &seeds, false, false);
            self.intern(state)
        };
        self.transitions[cell] = next.map_or(DEAD, |id| id as u32);
        next
    }

    fn final_match(&self, program: &Program, state_id: usize) -> bool {
        let pending = &self.states[state_id].pending_end;
        if pending.is_empty() {
            return false;
        }
        closure(program, pending, false, true).matched
    }

    fn intern(&mut self, state: StateData) -> Option<usize> {
        if let Some(&id) = self.index.get(&state) {
            return Some(id);
        }
        if self.states.len() >= self.limit {
            self.overflowed = true;
            return None;
        }
        if self
            .states
            .len()
            .saturating_add(1)
            .saturating_mul(self.num_classes)
            > MAX_TRANSITION_CELLS
        {
            self.overflowed = true;
            return None;
        }
        let id = self.states.len();
        self.matched.push(state.matched);
        self.index.insert(state.clone(), id);
        self.states.push(state);
        self.transitions
            .extend(std::iter::repeat_n(UNKNOWN, self.num_classes));
        Some(id)
    }

    pub(crate) fn overflowed(&self) -> bool {
        self.overflowed
    }
}

fn class_boundaries(program: &Program) -> Vec<u32> {
    let mut boundaries = vec![0, MAX_CODEPOINT + 1];
    for inst in &program.insts {
        match inst {
            Inst::Char(code_point) => {
                boundaries.push(*code_point);
                boundaries.push(code_point.saturating_add(1));
            }
            Inst::Class(class_id) => {
                for &(lo, hi) in program.classes[*class_id].ranges() {
                    boundaries.push(lo);
                    boundaries.push(hi.saturating_add(1).min(MAX_CODEPOINT + 1));
                }
            }
            _ => {}
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

fn build_map(boundaries: &[u32], length: usize) -> Vec<u16> {
    let mut map = vec![0u16; length];
    for (class_id, pair) in boundaries.windows(2).enumerate() {
        let lo = (pair[0] as usize).min(length);
        let hi = (pair[1] as usize).min(length);
        if lo < hi {
            map[lo..hi].fill(class_id as u16);
        }
    }
    map
}

fn instruction_matches(program: &Program, pc: usize, code_point: u32) -> bool {
    match program.insts[pc] {
        Inst::Char(expected) => expected == code_point,
        Inst::Class(class_id) => program.classes[class_id].contains(code_point),
        _ => false,
    }
}

/// Epsilon closure in priority order.
///
/// A `Match` prunes all lower-priority states. End assertions are kept pending
/// when the current position is not the end of input.
fn closure(program: &Program, seeds: &[usize], at_start: bool, at_end: bool) -> StateData {
    let mut state = StateData {
        consume: Vec::new(),
        pending_end: Vec::new(),
        matched: false,
    };
    let mut seen = vec![false; program.insts.len()];
    let mut stack: Vec<usize> = seeds.iter().rev().copied().collect();
    while let Some(pc) = stack.pop() {
        if seen[pc] {
            continue;
        }
        seen[pc] = true;
        match program.insts[pc] {
            Inst::Split(first, second) => {
                stack.push(second);
                stack.push(first);
            }
            Inst::Jump(target) => stack.push(target),
            // Capture slots are invisible to the DFA: it tracks group 0 only, so
            // `Save` is an epsilon edge and never becomes part of a state.
            Inst::Save(_) => {
                if pc + 1 < program.insts.len() {
                    stack.push(pc + 1);
                }
            }
            Inst::Assert(AssertKind::Start) => {
                if at_start && pc + 1 < program.insts.len() {
                    stack.push(pc + 1);
                }
            }
            // The cache declines these programs before reaching this point.
            Inst::Assert(AssertKind::EndLine)
            | Inst::Assert(AssertKind::StartLine)
            | Inst::Assert(AssertKind::EndAnyLine)
            | Inst::Assert(AssertKind::WordBoundary)
            | Inst::Assert(AssertKind::NotWordBoundary) => {}
            Inst::Assert(AssertKind::End) => {
                if at_end {
                    if pc + 1 < program.insts.len() {
                        stack.push(pc + 1);
                    }
                } else {
                    state.pending_end.push(pc);
                }
            }
            Inst::Char(_) | Inst::Class(_) => state.consume.push(pc),
            Inst::Match => {
                state.matched = true;
                stack.clear();
            }
        }
    }
    state
}

/// Search for the leftmost match using the first-unit prefilter and an
/// anchored DFA at each candidate.
pub(crate) fn find_candidates<U: Unit>(
    program: &Program,
    cache: &mut Cache,
    hay: &[U],
    search_start: usize,
    retry_nonempty_at: Option<usize>,
) -> FindResult {
    let Some(first) = program.first.as_ref() else {
        return FindResult::NoMatch;
    };
    let Some(scan) = first.scan::<U>() else {
        return FindResult::NoMatch;
    };
    let mut pos = search_start;
    loop {
        let Some(candidate) = scan.find_next(hay, pos) else {
            return FindResult::NoMatch;
        };
        match find_at(program, cache, hay, candidate, retry_nonempty_at) {
            FindResult::Match(found) => return FindResult::Match(found),
            FindResult::NoMatch => pos = candidate + 1,
            FindResult::Overflow => return FindResult::Overflow,
        }
    }
}

fn find_at<U: Unit>(
    program: &Program,
    cache: &mut Cache,
    hay: &[U],
    start: usize,
    retry_nonempty_at: Option<usize>,
) -> FindResult {
    let mut state_id = match cache.initial_state(start == 0, start == hay.len()) {
        Some(id) => id,
        None => return FindResult::Overflow,
    };
    let mut best = if cache.matched[state_id] && retry_nonempty_at != Some(start) {
        Some(start)
    } else {
        None
    };
    let mut pos = start;
    while pos < hay.len() {
        let unit: u32 = hay[pos].into();
        let Some(class_id) = cache.class_id(U::KIND, unit) else {
            return FindResult::Overflow;
        };
        let Some(next) = cache.transition(program, state_id, class_id) else {
            if cache.overflowed() {
                return FindResult::Overflow;
            }
            break;
        };
        state_id = next;
        pos += 1;
        if cache.matched[state_id] {
            best = Some(pos);
        }
    }
    if pos == hay.len() && cache.final_match(program, state_id) {
        best = Some(pos);
    }
    match best {
        Some(end) => FindResult::Match(Match::new(start, end)),
        None => FindResult::NoMatch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::compile;

    #[test]
    fn class_maps_partition_the_alphabet() {
        let program = compile(r"@[a-zA-Z][a-zA-Z0-9_]{4,31}").unwrap();
        let cache = Cache::new(&program).unwrap();
        assert_eq!(
            cache.class_id(UcsKind::Ucs1, u32::from(b'@')),
            cache.class_id(UcsKind::Ucs1, u32::from(b'@'))
        );
        assert_ne!(
            cache.class_id(UcsKind::Ucs1, u32::from(b'@')),
            cache.class_id(UcsKind::Ucs1, u32::from(b'a'))
        );
        assert_eq!(
            cache.class_id(UcsKind::Ucs4, 0x1F600),
            cache.class_id(UcsKind::Ucs4, 0x1F601)
        );
    }

    #[test]
    fn overflow_is_reported() {
        let program = compile("(a|b|c|d|e|f|g|h)+").unwrap();
        let mut cache = Cache::new_with_limit(&program, 1).unwrap();
        let result = find_candidates(&program, &mut cache, b"abcdefgh", 0, None);
        assert!(matches!(result, FindResult::Overflow));
        assert!(cache.overflowed());
    }
}
