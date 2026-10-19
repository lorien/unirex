"""Record failing tests and their messages, to triage the conformance ledger.

Run with `-p ledger_plugin` and `UNIREX_LEDGER_OUT=<path>`; writes one
`Class.test<TAB>message` line per failing test.
"""

import os

_results = []


def pytest_runtest_makereport(item, call):
    if call.excinfo is None:
        return
    if call.when != "call":
        return
    message = str(call.excinfo.value).splitlines()[0]
    _results.append((item.nodeid, message))


def pytest_sessionfinish(session, exitstatus):
    path = os.environ.get("UNIREX_LEDGER_OUT")
    if not path:
        return
    with open(path, "w", encoding="utf-8") as out:
        for nodeid, message in _results:
            test_id = nodeid.split("::", 1)[1] if "::" in nodeid else nodeid
            out.write(f"{test_id}\t{message}\n")
