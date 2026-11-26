"""Per-test temp dirs that clean up after themselves (docs/testing.md)."""

import tempfile


def scratch_dir(test, prefix="tmp-"):
    """A fresh temp dir for `test`, removed when the test finishes (works on Python 3.9)."""
    td = tempfile.TemporaryDirectory(prefix=prefix)
    test.addCleanup(td.cleanup)
    return td.name
