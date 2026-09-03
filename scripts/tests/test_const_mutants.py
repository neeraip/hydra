"""The decisions `const_mutants.py` makes before it runs anything.

Finding the constants and choosing their mutations is all the judgement in
the tool; the rest is `cargo test` in a loop. These pin the judgement so a
target can never be dropped silently — a constant this misses is one the
reviews would have to find by hand again.
"""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

from const_mutants import find_constants, mutate_value, strip_test_modules


class TestStripTestModules(unittest.TestCase):
    SRC = """\
const REAL: f64 = 1.0;

#[cfg(test)]
mod tests {
    const FIXTURE: f64 = 2.0;
    fn helper() { if x { y } }
}

const ALSO_REAL: u32 = 3;
"""

    def test_a_test_modules_constants_are_not_targets(self):
        names = [c.name for c in find_constants(self.SRC)]
        self.assertEqual(["REAL", "ALSO_REAL"], names)

    def test_offsets_survive_stripping_so_edits_land_in_the_original(self):
        stripped = strip_test_modules(self.SRC)
        self.assertEqual(len(self.SRC), len(stripped))
        # The value the tool will overwrite is where it says it is.
        c = find_constants(self.SRC)[0]
        self.assertEqual("1.0", self.SRC[c.start : c.end])

    def test_a_nested_brace_does_not_end_the_test_module_early(self):
        # `helper` closes a brace inside the module; ALSO_REAL is after it
        # and must still be found, which only holds if the scan counted
        # depth rather than stopping at the first `}`.
        self.assertIn("ALSO_REAL", [c.name for c in find_constants(self.SRC)])


class TestFindConstants(unittest.TestCase):
    def test_visibility_and_indent_do_not_hide_a_constant(self):
        src = (
            "pub const A: u32 = 1;\n"
            "pub(crate) const B: f64 = 2.0;\n"
            "pub(super) const C: f64 = 3.0;\n"
            "    const D: usize = 4;\n"
        )
        self.assertEqual(["A", "B", "C", "D"], [c.name for c in find_constants(src)])

    def test_an_expression_value_is_captured_whole(self):
        # A real one: a thousandth of a foot in metres.
        c = find_constants("const XTOL: f64 = 0.001 * 0.3048;\n")[0]
        self.assertEqual("0.001 * 0.3048", c.value)

    def test_a_value_spanning_lines_is_skipped_rather_than_guessed_at(self):
        src = "const TABLE: [f64; 2] = [\n    1.0,\n    2.0,\n];\n"
        self.assertEqual([], find_constants(src))

    def test_the_line_number_points_at_the_constant(self):
        src = "// a\n// b\nconst X: u32 = 1;\n"
        self.assertEqual(3, find_constants(src)[0].line)


class TestMutateValue(unittest.TestCase):
    def test_a_float_moves_three_orders_of_magnitude(self):
        self.assertEqual("(1.0e-5) * 1000.0", mutate_value("f64", "1.0e-5"))

    def test_a_float_expression_is_scaled_as_a_whole(self):
        # Without the parentheses this would scale only the last factor.
        self.assertEqual("(0.001 * 0.3048) * 1000.0", mutate_value("f64", "0.001 * 0.3048"))

    def test_a_zero_takes_a_magnitude_because_scaling_it_changes_nothing(self):
        for zero in ("0.0", "0", "0.000", "0.0e0"):
            self.assertEqual("1.0", mutate_value("f64", zero), zero)

    def test_an_integer_gains_one_so_a_count_really_changes(self):
        self.assertEqual("(3) + 1", mutate_value("u32", "3"))
        self.assertEqual("(200) + 1", mutate_value("usize", "200"))

    def test_a_bool_inverts(self):
        self.assertEqual("!(true)", mutate_value("bool", "true"))

    def test_types_with_no_single_obvious_change_are_skipped(self):
        for ty in ("&str", "[f64; 3]", "(usize, usize)", "char", "Duration"):
            self.assertIsNone(mutate_value(ty, "whatever"), ty)


if __name__ == "__main__":
    unittest.main()
