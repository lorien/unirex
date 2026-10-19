# unirex (name reserved)

This is a placeholder crate. It reserves the crates.io name `unirex` for an
experimental linear-time regular-expression engine that matches directly
against CPython's PEP 393 UCS-1, UCS-2, and UCS-4 string storage, with no UTF-8
conversion in the matching path.

**No code is published yet.** Version `0.0.1` has no functions, types, or
constants, so depending on it gains nothing. That is deliberate: the
placeholder must never offer an API that a later release has to keep.

The functional crate starts at `0.1.0` or later and is published from the
repository root, where the engine, the prefilter, the lazy DFA, and the Python
binding live. The PyPI distribution `unirex` is reserved the same way. This
placeholder will be yanked once the first functional release ships.

The project is not yet public. There is no support commitment and no API
guarantee. The MIT license in `LICENSE` applies to this crate's placeholder
content.
