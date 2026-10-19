# unirex (name reserved)

This is a placeholder release. It reserves the PyPI name `unirex` for an
experimental linear-time regular-expression engine that matches directly
against CPython's PEP 393 UCS-1, UCS-2, and UCS-4 string storage, with no UTF-8
conversion in the matching path.

**No code is published yet.** This distribution installs no modules, so

    import unirex

fails with `ModuleNotFoundError`. That is deliberate: the placeholder must
never shadow the real extension module when it ships.

Version `0.0.1` contains no functionality and is not intended for use. The
first functional release will be `0.1.0` or later and will provide a `unirex`
extension module built with PyO3 and maturin. This placeholder will be yanked
at that point.

The project is not yet public. There is no support commitment, no API
guarantee, and no license grant beyond the MIT license recorded in the
distribution metadata.
