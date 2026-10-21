## Report on task: Close the CPython-conformance gap with rurex

Ad-hoc task requested by the owner: unirex passed fewer of CPython's vendored
`test_re.py` methods than the neighbouring `rurex` project (60 vs 71 of 165 on
the 3.13.5 suite, both ledgers enforced); find why and close the gap.

### Done

- Re-measured the gap and corrected the initial diagnosis: pytest reports a
  unittest method whose `subTest` assertions fail as `PASSED` at method
  level, so the 14 "stale" ledger entries found by a naive outcome scan were
  real failures (subtest-level message mismatches). No entry was removed on
  that false premise; entries were removed only after methods passed with
  every subtest.
- Pre-validation scan (`src/compile.rs::prevalidate`) mirroring CPython's
  left-to-right parse checks for what `regex-syntax` cannot parse far enough
  to diagnose: global inline flag placement (3.11+ rules, including inside
  groups and conditionals, with verbose whitespace and `#` comments), and
  conditional groups `(?(ref)...)` (`bad group number`, `invalid group
  reference` against `_sre.MAXGROUPS` and against the final group count,
  `bad character in group name`, `unknown group name`,
  `conditional backref with more than two branches`, unterminated forms).
  This also fixed two divergences: `a(?i:)` and `((?i:))` are now accepted
  (scoped groups are legal anywhere), and `(?:(?i)X)` is rejected.
- `multiple repeat` position parity: the reported position is past the
  innermost operator plus one absorbed `?`/`+` modifier, accounting for
  regex-syntax merging lazy markers into operator spans. The full
  120-pattern matrix from `test_multiple_repeat` now matches CPython
  exactly.
- Replacement templates now mirror `re._parser.parse_template` in
  `src/python.rs`: parsed once before matching, `unirex.error` with CPython's
  message and `pos` for `invalid group reference`, octal range overflow, and
  `bad escape`; `IndexError` for unknown group names; literal unknown
  non-letter escapes; template validated even when the subject has no
  matches.
- Pattern value semantics: `__eq__`/`__hash__` by (pattern, bytesness,
  flags) so equal patterns hash alike after `purge`; ordering still raises;
  the comparison never mixes `str` and `bytes` values. `Match.__copy__` and
  `__deepcopy__` return the match itself.
- Buffer-protocol subjects: `bytearray`/`memoryview` accepted across
  `search`, `match`, `fullmatch`, `findall`, `finditer`, `split`, `sub`,
  `subn`; a match keeps the original object and reads groups live at call
  time with spans clamped to the current length, so a mutated `bytearray` is
  reflected as in `re` (`test_bug_29444`); `fullmatch` on buffers no longer
  degrades to `search`.
- Scanner: `Pattern.scanner(string, pos, endpos)` matches carry absolute
  spans honoring the window; a module-level `Scanner(lexicon, flags)` class
  mirrors CPython's compound-pattern scanner with `scan()`; the conformance
  shim now exposes it.
- Anchored-start fast rejection (ADR-0019): a compile-time instruction
  fixpoint marks programs whose every accepting run crosses a start
  assertion; unanchored searches over them try only the first position and
  skip the first-unit prefilter, and `split`/`sub` build untouched spans by
  `PyUnicode_Substring` instead of copying the haystack through a `String`.
  `test_search_anchor_at_beginning`'s ten operations now take about 1 ms.
- Ledger: 12 entries removed after the methods passed with all subtests
  (105 to 93); four stale reasons corrected. The 3.13.5 ledger-on run is
  clean: 72 passed, 93 skipped, zero failures — ahead of rurex's 71 on the
  same tag.
- Checks: `cargo fmt`, `clippy -D warnings`, `cargo test` (debug and
  release, 23 unit tests including new ones for the pre-scan, anchoring,
  and repeat positions), binding smoke test, `python_diff.py`
  (127,955 cases, 0 mismatches), bench self-test, 100k-post end-to-end
  benchmark (efficiency 1.00, 43,456 mentions), docs check, unicode table
  check, publish dry-run, conformance suite, and the re guard.

### Spec/ADR amendments

- [acted] Added ADR-0019 (anchored-start fast rejection) and catalogued it in
  `spec/docs/index.md`.
- [acted] `spec/docs/semantics.md`: documented exact global-flag placement,
  conditional diagnostics, the possessive `multiple repeat` position,
  pattern value equality, match copying, buffer subjects with live reads,
  template parity, the Scanner surface, and the anchored search guarantee.
- [acted] `spec/docs/testing.md` layer 14: refreshed ledger numbers
  (93 of 165; 57 unsupported / 30 todo / 2 by-design / 4 harness), the
  rurex comparison, and the subtest-vs-method counting caveat.

### Future-task notes

- [open] Error-message parity for `regex-syntax` parse errors
  (`test_misc_errors`, `test_symbolic_groups_errors`,
  `test_character_set_errors`, `test_nothing_to_repeat`,
  `test_other_escapes`, `test_sre_character_literals`,
  `test_sre_character_class_literals`, the external suites, and the two
  `*_group_name_in_exception` tests): these methods pass at pytest level
  but individual subtests compare CPython messages. A mapping layer over
  `regex_syntax::Error` kinds would close them; `Error::Parse` messages
  also show a doubled `regex parse error:` prefix that would surface there.
- [open] Possessive quantifiers stay rejected as stacked repetition
  (`test_fullmatch_possessive_quantifiers`, `test_bug_gh101955`); rurex
  passes them by parsing `a*+` as `(a*)+`, which coincides with possessive
  semantics on these tests but not in general. Supporting them for real
  needs cut semantics in the NFA or a bounded backtrack.
- [open] `(?#...)` group comments are rejected by `regex-syntax` (noted in
  `semantics.md` since before this work); CPython accepts them.
- [open] `Pattern.is_match` still takes a `str` only; buffers and bytes
  subjects go through the other methods.

### Tooling/process

- [open] `tests/cpython/ledger_plugin.py` records only hard failures; a
  `SUBFAILED`-aware triage mode would make ledger regeneration resist the
  method-level-PASSED pitfall documented in `testing.md`.
- The pyo3 buffer API's `as_slice` is deprecated and unsound-flagged in
  0.30; the binding uses `as_slice_ptr` inside a documented safety block.
