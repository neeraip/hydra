"""No tracked text file may carry a raw control byte.

`undoStack.ts` separated its stack keys with a literal NUL. The value was
right (no id can contain one, so no two pairs collide) but the byte was
written into the source rather than escaped, and grep classifies any file
holding a NUL as binary. So `grep -rn "export" undoStack.ts` printed
nothing, and every text sweep of this repository - the review's audits
among them - silently skipped a 773-line file for as long as it was
there. Nothing failed, which is exactly the problem.

Escapes cost nothing at runtime and keep the file readable by every tool.
"""

import pathlib
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

# Listed by suffix rather than detected from content: a file's bytes cannot
# be the test of whether its bytes are allowed, which is what let the NUL in
# `undoStack.ts` look like an ordinary binary file to git.
BINARY_SUFFIXES = {
    ".png", ".ico", ".icns", ".jpg", ".jpeg", ".gif", ".pdf", ".wasm",
    ".woff", ".woff2", ".ttf", ".otf", ".zip", ".gz",
    # Reference interface files written by the predecessor itself, kept so
    # this engine's readers are checked against the formats rather than
    # against their own writers (uds §14.8.2, §14.8.3).
    ".rff", ".rain",
}

# Tab, newline and carriage return are the control bytes text is made of.
ALLOWED = {0x09, 0x0A, 0x0D}


def tracked_text_files() -> list[pathlib.Path]:
    out = subprocess.run(
        ["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout
    return [
        ROOT / name
        for name in out.split("\0")
        if name and pathlib.Path(name).suffix.lower() not in BINARY_SUFFIXES
    ]


class GreppableSourceTests(unittest.TestCase):
    def test_no_tracked_text_file_carries_a_raw_control_byte(self):
        offences = []
        for path in tracked_text_files():
            data = path.read_bytes()
            for offset, byte in enumerate(data):
                if byte < 0x20 and byte not in ALLOWED:
                    line = data[:offset].count(b"\n") + 1
                    rel = path.relative_to(ROOT)
                    offences.append(f"{rel}:{line}: byte {byte:#04x}")
                    break
        self.assertEqual(
            [],
            offences,
            "a raw control byte makes the whole file binary to grep, which "
            "silently drops it from every text search; write it as an escape",
        )

    def test_the_guard_reads_the_file_that_regressed(self):
        looked = {p.relative_to(ROOT).as_posix() for p in tracked_text_files()}
        self.assertIn("crates/gui/frontend/src/hooks/undoStack.ts", looked)

    def test_the_guard_would_catch_a_nul(self):
        # Proving the scan is not vacuously green.
        data = b'const k = `a\x00b`;\n'
        self.assertTrue(any(b < 0x20 and b not in ALLOWED for b in data))


if __name__ == "__main__":
    unittest.main()
