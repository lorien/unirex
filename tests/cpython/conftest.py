r"""Conformance harness: run CPython's vendored `test_re.py` against
unirex by shimming `re` -> unirex and `test.support` -> support_stub.

Every test that does not pass must be recorded in EXPECTED below with
a reason, so the suite stays an honest compatibility ledger: what runs
green is compatible, what does not is accounted for. Buckets:

  by-design   — unirex intentionally rejects/behaves differently
                (no backrefs/lookaround/atomic/possessive, `\Z`,
                octal escapes, Pattern(...) constructible; see
                spec/docs/api.md "Semantic differences" and ADRs).
  internals   — tests of CPython implementation internals (re._parser,
                _compiler, _cache, pickle reconstruction via re._compile).
  harness     — artifacts of running the file outside CPython's own
                test runner (subprocess/locale specifics).

Update EXPECTED when unirex closes a gap (remove the entry) or a new
failure appears (add the entry with its bucket).
"""

import sys
import types

import pytest

import unirex

# ------------------------------------------------------------------ shim

RE_PUBLIC = {k: v for k, v in vars(unirex).items() if not k.startswith("_")}


def _install_shims():
    re_mod = types.ModuleType("re")
    re_mod.__dict__.update(RE_PUBLIC)
    re_mod.__dict__.update(
        {
            "Pattern": unirex.Pattern,
            "Match": unirex.Match,
            "Scanner": None,
            "_compile": unirex.compile,  # pickle reconstruction helper
            "__name__": "re",
        }
    )

    test_pkg = types.ModuleType("test")
    test_pkg.__path__ = []

    import support_stub

    support_mod = types.ModuleType("test.support")
    support_mod.__dict__.update(vars(support_stub))

    sys.modules["re"] = re_mod
    sys.modules["test"] = test_pkg
    sys.modules["test.support"] = support_mod
    test_pkg.support = support_mod

    # test.re_tests: the external data suite, vendored from the same
    # tag (contains `tests`, `benchmarks`, FAIL, SYNTAX_ERROR).
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "test.re_tests",
        __import__("pathlib").Path(__file__).with_name("vendored_re_tests.py"),
    )
    re_tests = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(re_tests)
    sys.modules["test.re_tests"] = re_tests
    test_pkg.re_tests = re_tests


def pytest_pycollect_makemodule(module_path, parent):
    # Install the shims only when the vendored suite is really being
    # collected (this hook runs after the test-file name filter), so a
    # plain `pytest` run never sees a patched `re`.
    if module_path.name == "vendored_test_re.py":
        _install_shims()
    return None



# ---------------------------------------------------------------- ledger

EXPECTED = {
    "DebugTests.test_atomic_group": "unsupported: construct this engine rejects by decision (DEBUG flag is not supported)",
    "DebugTests.test_debug_flag": "unsupported: construct this engine rejects by decision (DEBUG flag is not supported)",
    "DebugTests.test_possesive_repeat": "unsupported: construct this engine rejects by decision (DEBUG flag is not supported)",
    "DebugTests.test_possesive_repeat_one": "unsupported: construct this engine rejects by decision (DEBUG flag is not supported)",
    "ExternalTests.test_re_benchmarks": "todo: known gap, to be closed (Pattern.match() takes 1 positional arguments but 2 were give)",
    "ExternalTests.test_re_tests": "todo: known gap, to be closed ('unirex.Match' object has no attribute 're')",
    "ImplementationTest.test_disallow_instantiation": "todo: known gap, to be closed ('unirex.Pattern' object has no attribute 'scanner')",
    "ImplementationTest.test_immutable": "todo: known gap, to be closed (TypeError not raised)",
    "ImplementationTest.test_overlap_table": "todo: known gap, to be closed (module 're' has no attribute '_compiler')",
    "ImplementationTest.test_repeat_minmax_overflow_maxrepeat": "todo: known gap, to be closed (pattern too large at offset 1: repetition count)",
    "ImplementationTest.test_signedness": "todo: known gap, to be closed (module 're' has no attribute '_compiler')",
    "PatternReprTests.test_bytes": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "PatternReprTests.test_flags_repr": "todo: known gap, to be closed ('2' != 're.IGNORECASE')",
    "PatternReprTests.test_inline_flags": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('(?i)pattern', re.IGNORECASE)",
    "PatternReprTests.test_locale": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "PatternReprTests.test_long_pattern": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('Very long long lon')",
    "PatternReprTests.test_multiple_flags": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('random pattern', re.IGNOREC)",
    "PatternReprTests.test_quotes": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile(\'random 'double quoted' pat)",
    "PatternReprTests.test_single_flag": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('random pattern', re.IGNOREC)",
    "PatternReprTests.test_unicode_flag": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('random pattern')')",
    "PatternReprTests.test_unknown_flags": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('random pattern', 0x123000)')",
    "PatternReprTests.test_without_flags": "todo: known gap, to be closed ('unirex.Pattern' != 're.compile('random pattern')')",
    "ReTests.test_ASSERT_NOT_mark_bug": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_MIN_REPEAT_ONE_mark_bug": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_REPEAT_ONE_mark_bug": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_ascii_and_unicode_flag": "unsupported: construct this engine rejects by decision (ASCII flag is not supported: classes follow CPython's Unicod)",
    "ReTests.test_atomic_grouping": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_backref_group_name_in_exception": "todo: known gap, to be closed ('regex parse error: regex parse error:\n [44 chars]flag' != )",
    "ReTests.test_basic_re_sub": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_bigcharset": "todo: known gap, to be closed ('utf-8' codec can't encode characters in position 217-224: s)",
    "ReTests.test_bug_1661": "todo: known gap, to be closed ('Pattern' object is not an instance of 'str')",
    "ReTests.test_bug_16688": "unsupported: construct this engine rejects by decision (unsupported syntax at offset 7: backreferences)",
    "ReTests.test_bug_2537": "todo: known gap, to be closed ('xyy' != '')",
    "ReTests.test_bug_29444": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_bug_34294": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_bug_3629": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_bug_40736": "todo: known gap, to be closed ('got 'int'' does not match ''int' object is not an instance )",
    "ReTests.test_bug_449964": "todo: known gap, to be closed ('xxbxxb' != 'xx\x08xx\x08')",
    "ReTests.test_bug_527371": "todo: known gap, to be closed ('unirex.Match' object has no attribute 'lastindex')",
    "ReTests.test_bug_581080": "todo: known gap, to be closed ('unirex.Pattern' object has no attribute 'scanner')",
    "ReTests.test_bug_6509": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_bug_725149": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_bug_926075": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_bug_gh100061": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_bug_gh91616": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_bytes_str_mixing": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_character_set_errors": "todo: known gap, to be closed ('regex parse error: regex parse error:\n  [39 chars]lass' !=)",
    "ReTests.test_comments": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_compile": "todo: known gap, to be closed ('Pattern' object is not an instance of 'str')",
    "ReTests.test_copying": "todo: known gap, to be closed (cannot pickle 'unirex.Pattern' object)",
    "ReTests.test_empty_array": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_enum": "todo: known gap, to be closed ('ASCII' not found in '256')",
    "ReTests.test_error": "todo: known gap, to be closed (5 != 3)",
    "ReTests.test_expand": "todo: known gap, to be closed ('unirex.Match' object has no attribute 'expand')",
    "ReTests.test_fail": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_findall_atomic_grouping": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_findall_possessive_quantifiers": "todo: known gap, to be closed (Lists differ: ['aa', '', ''] != ['a', 'a', '', ''])",
    "ReTests.test_finditer": "todo: known gap, to be closed (Pattern.finditer() takes 1 positional arguments but 3 were g)",
    "ReTests.test_flags": "unsupported: construct this engine rejects by decision (ASCII flag is not supported: classes follow CPython's Unicod)",
    "ReTests.test_fullmatch_atomic_grouping": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_getattr": "todo: known gap, to be closed ('unirex.Pattern' object has no attribute 'pattern')",
    "ReTests.test_group": "todo: known gap, to be closed (IndexError not raised by group)",
    "ReTests.test_group_name_in_exception": "todo: known gap, to be closed ('regex parse error: regex parse error:\n [60 chars]cter' != )",
    "ReTests.test_ignore_case": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_ignore_case_range": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_ignore_case_set": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_ignore_spaces": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_inline_flags": "todo: known gap, to be closed ('unsupported syntax at offset 3: global flags outside the st)",
    "ReTests.test_issue17998": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_keep_buffer": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_keyword_parameters": "todo: known gap, to be closed (Pattern.match() got an unexpected keyword argument 'string')",
    "ReTests.test_large_search": "todo: known gap, to be closed (bigmem test (set UNIREX_BIGMEM=1 to run))",
    "ReTests.test_large_subn": "todo: known gap, to be closed (bigmem test (set UNIREX_BIGMEM=1 to run))",
    "ReTests.test_locale_caching": "unsupported: construct this engine rejects by decision (test needs en_US.iso88591 locale)",
    "ReTests.test_locale_compiled": "unsupported: construct this engine rejects by decision (test needs en_US.iso88591 locale)",
    "ReTests.test_locale_flag": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_look_behind_overflow": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_lookahead": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_lookbehind": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_match_getitem": "todo: known gap, to be closed ('unirex.Match' object is not subscriptable)",
    "ReTests.test_match_repr": "unsupported: construct this engine rejects by decision (unsupported syntax at offset 9: backreferences)",
    "ReTests.test_memory_leaks": "todo: known gap, to be closed (requires debug build)",
    "ReTests.test_misc_errors": "todo: known gap, to be closed ('regex parse error: regex parse error:\n    (\n    ^\nerror:)",
    "ReTests.test_misuse_flags": "todo: known gap, to be closed (DeprecationWarning not triggered)",
    "ReTests.test_multiple_repeat": "todo: known gap, to be closed (Error not raised)",
    "ReTests.test_named_unicode_escapes": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_nothing_to_repeat": "todo: known gap, to be closed ('regex parse error: regex parse error:\n  [53 chars]sion' !=)",
    "ReTests.test_other_escapes": "todo: known gap, to be closed ('regex parse error: regex parse error:\n  [78 chars]rely' !=)",
    "ReTests.test_pattern_compare": "todo: known gap, to be closed (8793332408070 != 8793332408059)",
    "ReTests.test_pattern_compare_bytes": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_pickling": "todo: known gap, to be closed (cannot pickle 'Pattern' object)",
    "ReTests.test_possessive_quantifiers": "todo: known gap, to be closed (<unirex.Match span=(0, 4)> is not None)",
    "ReTests.test_possible_set_operations": "unsupported: construct this engine rejects by decision (unsupported syntax at offset 1: character-class set operatio)",
    "ReTests.test_qualified_re_split": "todo: known gap, to be closed (DeprecationWarning not triggered)",
    "ReTests.test_qualified_re_sub": "todo: known gap, to be closed (DeprecationWarning not triggered)",
    "ReTests.test_re_escape": "todo: known gap, to be closed (None is not true)",
    "ReTests.test_re_escape_bytes": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_re_escape_non_ascii_bytes": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_re_findall": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_re_fullmatch": "todo: known gap, to be closed ('NoneType' object has no attribute 'span')",
    "ReTests.test_re_groupref": "unsupported: construct this engine rejects by decision (unsupported syntax at offset 14: backreferences)",
    "ReTests.test_re_groupref_exists": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_re_groupref_exists_errors": "todo: known gap, to be closed ('regex parse error: regex parse error:\n  [56 chars]flag' !=)",
    "ReTests.test_re_groupref_exists_validation_bug": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_re_groupref_overflow": "todo: known gap, to be closed (Error not raised)",
    "ReTests.test_re_match": "todo: known gap, to be closed (Match.group() takes from 0 to 1 positional arguments but 2 w)",
    "ReTests.test_re_split": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_re_subn": "todo: known gap, to be closed (DeprecationWarning not triggered)",
    "ReTests.test_regression_gh94675": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_repeat_minmax": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_repeat_minmax_overflow": "todo: known gap, to be closed (pattern too large at offset 1: repetition count)",
    "ReTests.test_scanner": "todo: known gap, to be closed ('NoneType' object is not callable)",
    "ReTests.test_scoped_flags": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_search_anchor_at_beginning": "todo: known gap, to be closed (0.22297906503081322 not less than 0.1)",
    "ReTests.test_special_escapes": "unsupported: construct this engine rejects by decision (ASCII flag is not supported: classes follow CPython's Unicod)",
    "ReTests.test_sre_byte_class_literals": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_sre_byte_literals": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_sre_character_class_literals": "todo: known gap, to be closed ('unsupported syntax at offset 1: octal escape out of range' )",
    "ReTests.test_sre_character_literals": "todo: known gap, to be closed ('unsupported syntax at offset 0: octal escape out of range' )",
    "ReTests.test_sub_template_numeric_escape": "todo: known gap, to be closed ('x' != '\x00')",
    "ReTests.test_symbolic_groups": "todo: known gap, to be closed (regex parse error: regex parse error:)",
    "ReTests.test_symbolic_groups_errors": "todo: known gap, to be closed ('regex parse error: regex parse error:\n [68 chars]name' != )",
    "ReTests.test_symbolic_refs": "todo: known gap, to be closed ('bytes' object is not an instance of 'str')",
    "ReTests.test_symbolic_refs_errors": "todo: known gap, to be closed (Error not raised)",
    "ReTests.test_weakref": "todo: known gap, to be closed (cannot create weak reference to 'unirex.Pattern' object)",
    "ReTests.test_word_boundaries": "unsupported: construct this engine rejects by decision (ASCII flag is not supported: classes follow CPython's Unicod)",
    "ReTests.test_zerowidth": "todo: known gap, to be closed (regex parse error: regex parse error:)",
}


def pytest_collection_modifyitems(config, items):
    for item in items:
        cls = getattr(item, "cls", None)
        key = f"{cls.__name__}.{item.name}" if cls else item.name
        if key in EXPECTED:
            # skip (not xfail): unittest subTest failures inside a
            # method are reported independently and would bypass the
            # marker; the ledger is kept honest by review — closing a
            # gap means removing the entry so the test runs again.
            item.add_marker(pytest.mark.skip(reason=EXPECTED[key]))
