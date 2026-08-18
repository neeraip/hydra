"""The GUI's command names have to exist on both sides.

`PatternPreview` asked for `get_patterns` for as long as it existed. No
Rust command of that name was ever written, so the invoke rejected, and
`tryInvoke` turned the rejection into `null` and then into an empty list.
The component read the empty list as a dangling pattern reference and
returned `null`, which is exactly what it should do when a reference
really is dangling. Nothing threw, nothing was logged where anyone would
look, and the chart simply never drew.

Nothing in either language could catch that: TypeScript sees a string,
Rust sees a function nobody calls. Only comparing the two lists does.

Both directions are checked. A name the frontend invokes and the backend
does not serve is the defect above. A command the backend registers and
the frontend never names is dead weight in the surface, and the same
comparison finds it for free.
"""

import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
FRONTEND = ROOT / "crates" / "gui" / "frontend" / "src"
MAIN = ROOT / "crates" / "gui" / "src" / "main.rs"

INVOKE = re.compile(
    r'(?:tryInvokeResult|tryInvokeOr|tryInvoke|invoke)(?:<[^(]*?>)?\(\s*"([a-z_0-9]+)"'
)
# An invoke whose first argument is not a literal. Each one has to be
# accounted for, or the forward check silently stops covering call sites.
DYNAMIC = re.compile(
    r'(?:tryInvokeResult|tryInvokeOr|tryInvoke|invoke)(?:<[^(]*?>)?\(\s*[A-Za-z_$]'
)
# Where a non-literal first argument is correct, and why.
DYNAMIC_ALLOWED = {
    # The wrappers themselves: `cmd` is their own parameter.
    "hooks/ipc.ts",
    # Picks between two literals in a ternary. Both are registered, and
    # the backward check below is what covers them.
    "components/modals/CommandPalette.tsx",
}


def frontend_sources() -> list[pathlib.Path]:
    files = sorted([*FRONTEND.rglob("*.ts"), *FRONTEND.rglob("*.tsx")])
    return [p for p in files if ".test." not in p.name]


def registered_commands() -> set[str]:
    text = MAIN.read_text()
    start = text.index("generate_handler!")
    block = text[start : text.index("]", start)]
    return set(re.findall(r"commands::([a-z_0-9]+)", block))


def invoked_commands() -> set[str]:
    names: set[str] = set()
    for path in frontend_sources():
        names |= set(INVOKE.findall(path.read_text()))
    return names


class CommandSurfaceTests(unittest.TestCase):
    def test_every_invoked_command_is_registered(self):
        missing = sorted(invoked_commands() - registered_commands())
        self.assertEqual(
            [],
            missing,
            "the frontend invokes these, and main.rs registers no such command; "
            "the invoke rejects at runtime and tryInvoke swallows it",
        )

    def test_every_registered_command_is_named_by_the_frontend(self):
        text = "\n".join(p.read_text() for p in frontend_sources())
        unused = sorted(c for c in registered_commands() if f'"{c}"' not in text)
        self.assertEqual(
            [],
            unused,
            "registered but never called; remove it or wire it up",
        )

    def test_every_dynamic_call_site_is_accounted_for(self):
        # Without this the forward check would quietly cover less and less
        # as call sites move to computed names.
        found = set()
        for path in frontend_sources():
            if DYNAMIC.search(path.read_text()):
                found.add(path.relative_to(FRONTEND).as_posix())
        self.assertEqual(
            DYNAMIC_ALLOWED,
            found,
            "an invoke with a computed command name cannot be checked against "
            "main.rs; add it to DYNAMIC_ALLOWED with the reason it is safe",
        )

    def test_the_lists_are_not_empty(self):
        # A regex that stops matching would otherwise pass everything.
        self.assertGreater(len(registered_commands()), 50)
        self.assertGreater(len(invoked_commands()), 50)


if __name__ == "__main__":
    unittest.main()
