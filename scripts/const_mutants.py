"""Change a constant, run the tests, and report the ones nothing noticed.

Three reviews of this codebase found the same defect, and it was never in
the logic. `cargo mutants` replaces function *bodies*, so a value written
once and depended on everywhere is invisible to it: the water-distribution
barrier stiffness could move six orders of magnitude, the drainage
integrator's error tolerance three, and the graphical app's on-disk engine
default could change from `wds` to `uds` — opening every project written
before that field existed under the wrong engine — with every suite green.

Each of those is a number a specification fixes. Nothing compiled wrong,
no test failed, and no reviewer reading the diff would see anything but a
constant with a different value in it.

So this changes each constant in turn and asks whether anything fails. A
constant that survives is not necessarily a defect: some are genuinely
free, and the answer for those is a test that says so, or a comment
saying why not. What it must not be is unknown.

The mutation is deliberately crude. Floats scale by a thousand, integers
gain one, booleans invert. A subtler mutation would be a better test of
the tests, but a cruder one that still survives is a stronger result: if
tripling a tolerance changes nothing, no test is looking at it at all.
"""

import argparse
import pathlib
import re
import signal
import subprocess
import sys
from dataclasses import dataclass

REPO = pathlib.Path(__file__).resolve().parent.parent

# `const NAME: TYPE = VALUE;` at any indent, value on one line. Deliberately
# not a Rust parser: a constant whose value spans lines is skipped rather
# than guessed at, and skipping is reported.
CONST = re.compile(
    r"^(?P<indent>[ \t]*)(?:pub(?:\([^)]*\))?\s+)?const\s+"
    r"(?P<name>[A-Z_][A-Z0-9_]*)\s*:\s*(?P<ty>[^=\n]+?)\s*=\s*(?P<value>[^;\n]+);",
    re.MULTILINE,
)

FLOAT_TYPES = {"f32", "f64"}
INT_TYPES = {
    "u8", "u16", "u32", "u64", "u128", "usize",
    "i8", "i16", "i32", "i64", "i128", "isize",
}


@dataclass(frozen=True)
class Constant:
    """One `const` item and where its value sits in the file."""

    name: str
    ty: str
    value: str
    start: int
    end: int
    line: int


def strip_test_modules(text: str) -> str:
    """Blank out `#[cfg(test)]` modules, keeping every byte offset.

    Offsets are preserved because the caller locates constants in the
    stripped text and edits the original: a test module's constants are
    the test's own business, and mutating them proves nothing.
    """
    out = list(text)
    for m in re.finditer(r"#\[cfg\(test\)\]", text):
        brace = text.find("{", m.end())
        if brace == -1:
            continue
        depth, i = 0, brace
        while i < len(text):
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        for j in range(m.start(), min(i + 1, len(text))):
            if out[j] != "\n":
                out[j] = " "
    return "".join(out)


def find_constants(text: str) -> list[Constant]:
    """Every mutable-in-principle constant outside test modules."""
    stripped = strip_test_modules(text)
    found = []
    for m in CONST.finditer(stripped):
        found.append(
            Constant(
                name=m.group("name"),
                ty=m.group("ty").strip(),
                value=m.group("value").strip(),
                start=m.start("value"),
                end=m.end("value"),
                line=stripped.count("\n", 0, m.start()) + 1,
            )
        )
    return found


def mutate_value(ty: str, value: str) -> str | None:
    """A different value of the same type, or `None` if this one is skipped.

    Skipped: strings, chars, arrays, tuples and anything else whose
    "obviously different" has no single meaning. Those carry their own
    structure, and a mutation that changed one would be testing the
    harness rather than the suite.
    """
    if ty in FLOAT_TYPES:
        # A zero scales to itself, so it takes a magnitude instead.
        if re.fullmatch(r"0(?:\.0*)?(?:[eE][+-]?\d+)?", value.replace("_", "")):
            return "1.0"
        return f"({value}) * 1000.0"
    if ty in INT_TYPES:
        # Plus one: a real change to a count or a width, and it can only
        # overflow at exactly the type's maximum, which is reported as a
        # build error rather than mistaken for a caught mutation.
        return f"({value}) + 1"
    if ty == "bool":
        return f"!({value})"
    return None


@dataclass
class Result:
    """What happened when one constant was changed."""

    const: Constant
    path: pathlib.Path
    outcome: str  # "caught" | "survived" | "build-error"


def run_tests(package: str) -> tuple[bool, bool]:
    """`(compiled, tests_passed)` for one package."""
    proc = subprocess.run(
        ["cargo", "test", "-p", package],
        cwd=REPO,
        capture_output=True,
        text=True,
    )
    out = proc.stdout + proc.stderr
    compiled = "error: could not compile" not in out and "error[E" not in out
    passed = proc.returncode == 0
    return compiled, passed


def probe(path: pathlib.Path, const: Constant, new: str, package: str) -> str:
    """Apply one mutation, test, and always put the file back."""
    original = path.read_text()
    mutated = original[: const.start] + new + original[const.end :]

    def restore(*_):
        path.write_text(original)

    previous = signal.signal(signal.SIGINT, lambda *a: (restore(), sys.exit(130)))
    try:
        path.write_text(mutated)
        compiled, passed = run_tests(package)
        if not compiled:
            return "build-error"
        return "caught" if not passed else "survived"
    finally:
        restore()
        signal.signal(signal.SIGINT, previous)
        # The file must be byte-identical again. A stranded mutation reads
        # as an ordinary edit in `git status`, and this run would be the
        # thing that put it there.
        if path.read_text() != original:
            raise SystemExit(f"FATAL: could not restore {path}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("crate", help="crate directory, e.g. crates/engine-wds")
    ap.add_argument("--package", help="cargo package name (default: crate dir name)")
    ap.add_argument("--filter", default="", help="only constants whose name contains this")
    ap.add_argument("--list", action="store_true", help="list targets and stop")
    args = ap.parse_args(argv)

    crate = (REPO / args.crate).resolve()
    package = args.package or crate.name
    if not (crate / "src").is_dir():
        print(f"no src/ under {crate}", file=sys.stderr)
        return 2

    targets: list[tuple[pathlib.Path, Constant, str]] = []
    skipped = 0
    for path in sorted(crate.rglob("src/**/*.rs")):
        text = path.read_text()
        for const in find_constants(text):
            if args.filter and args.filter not in const.name:
                continue
            new = mutate_value(const.ty, const.value)
            if new is None:
                skipped += 1
                continue
            targets.append((path, const, new))

    rel = lambda p: p.relative_to(REPO)  # noqa: E731
    if args.list:
        for path, const, new in targets:
            print(f"{rel(path)}:{const.line}  {const.name}: {const.ty} = {const.value}  ->  {new}")
        print(f"\n{len(targets)} targets, {skipped} skipped (non-numeric)")
        return 0

    if not targets:
        print("no constants matched")
        return 0

    print(f"{len(targets)} constants in {package} ({skipped} skipped as non-numeric)\n")
    survived: list[Result] = []
    for i, (path, const, new) in enumerate(targets, 1):
        print(f"[{i}/{len(targets)}] {const.name:32} ", end="", flush=True)
        outcome = probe(path, const, new, package)
        print(outcome)
        if outcome == "survived":
            survived.append(Result(const, path, outcome))

    print()
    if not survived:
        print("every constant is held by a test")
        return 0
    print(f"{len(survived)} constants no test noticed changing:\n")
    for r in survived:
        print(f"  {rel(r.path)}:{r.const.line}  {r.const.name}: {r.const.ty} = {r.const.value}")
    print(
        "\nEach wants a test asserting its value, or a comment saying why it is free."
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
