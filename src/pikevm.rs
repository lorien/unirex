//! Linear-time PikeVM simulation over UCS code points.

use crate::Match;
use crate::compile::{AssertKind, Inst, Program};
use crate::dfa;
use crate::ucs::Unit;

/// Reusable PikeVM scratch buffers.
///
/// Create one with [`crate::Regex::scratch`] and reuse it across searches to
/// avoid allocating thread lists and the epsilon-closure stack per call.
/// A `Scratch` is not shared between threads; give each worker its own.
pub struct Scratch {
    clist: ThreadList,
    nlist: ThreadList,
    stack: Vec<(usize, usize)>,
    dfa: Option<dfa::Cache>,
    dfa_disabled: bool,
}

impl Scratch {
    pub(crate) fn new(program_len: usize) -> Scratch {
        Scratch {
            clist: ThreadList::new(program_len),
            nlist: ThreadList::new(program_len),
            stack: Vec::new(),
            dfa: None,
            dfa_disabled: false,
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
            add_thread(clist, stack, prog, prog.start, pos, pos, hay.len());
        }

        let mut index = 0;
        while index < clist.len() {
            let pc = clist.pcs[index];
            let start = clist.starts[index];
            match prog.insts[pc] {
                Inst::Char(expected) => {
                    if pos < hay.len() && hay[pos].into() == expected {
                        add_thread(nlist, stack, prog, pc + 1, start, pos + 1, hay.len());
                    }
                }
                Inst::Class(class_id) => {
                    if pos < hay.len() && prog.classes[class_id].contains(hay[pos].into()) {
                        add_thread(nlist, stack, prog, pc + 1, start, pos + 1, hay.len());
                    }
                }
                Inst::Match => {
                    let empty_at_retry = retry_nonempty_at == Some(pos) && start == pos;
                    if !empty_at_retry {
                        matched = Some(Match::new(start, pos));
                        break;
                    }
                }
                Inst::Split(_, _) | Inst::Jump(_) | Inst::Assert(_) => {
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

fn add_thread(
    list: &mut ThreadList,
    stack: &mut Vec<(usize, usize)>,
    prog: &Program,
    pc: usize,
    thread_start: usize,
    pos: usize,
    hay_len: usize,
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
                if assert_holds(kind, pos, hay_len) {
                    stack.push((pc + 1, start));
                }
            }
            Inst::Char(_) | Inst::Class(_) | Inst::Match => list.push(pc, start),
        }
    }
}

fn assert_holds(kind: AssertKind, pos: usize, len: usize) -> bool {
    kind.holds(pos, len)
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
}
