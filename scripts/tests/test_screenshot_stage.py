"""The decisions in screenshot-stage: engine recognition from section
headers, the stable path-derived project id, and the launch
environment's HOME swap."""

import importlib.util
import json
import pathlib
import unittest
import uuid
from unittest import mock

SCRIPT = pathlib.Path(__file__).resolve().parent.parent / "screenshot-stage.py"
spec = importlib.util.spec_from_file_location("screenshot_stage", SCRIPT)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


class SniffEngineTests(unittest.TestCase):
    def test_pipes_read_as_water_distribution(self):
        self.assertEqual(mod.sniff_engine("[TITLE]\n[JUNCTIONS]\n[PIPES]\n"), "wds")

    def test_subcatchments_read_as_drainage(self):
        self.assertEqual(mod.sniff_engine("[JUNCTIONS]\n[SUBCATCHMENTS]\n[CONDUITS]\n"), "uds")

    def test_shared_sections_alone_are_ambiguous(self):
        # [JUNCTIONS]/[CURVES]/[OPTIONS] exist in both formats; refusing
        # to guess is what routes the user to --engine.
        self.assertIsNone(mod.sniff_engine("[JUNCTIONS]\n[CURVES]\n[OPTIONS]\n"))

    def test_both_engines_markers_are_ambiguous(self):
        self.assertIsNone(mod.sniff_engine("[PIPES]\n[CONDUITS]\n"))

    def test_headers_are_matched_case_insensitively_and_indented(self):
        self.assertEqual(mod.sniff_engine("  [Pipes]\n"), "wds")

    def test_the_real_fixtures_recognise(self):
        wds = (mod.REPO / "tests" / "benchmarks" / "wds" / "ltown.inp").read_text(errors="replace")
        self.assertEqual(mod.sniff_engine(wds), "wds")
        uds = (mod.REPO / "tests" / "fixtures" / "uds" / "runoff_parcel.inp").read_text(errors="replace")
        self.assertEqual(mod.sniff_engine(uds), "uds")


class ProjectIdTests(unittest.TestCase):
    def test_the_same_path_always_maps_to_the_same_project(self):
        # Stability is the point: staged runs and prefs must survive
        # restaging, which they only do if the id never moves.
        a = mod.project_id_for(pathlib.Path("/models/city.inp"))
        self.assertEqual(a, mod.project_id_for(pathlib.Path("/models/city.inp")))
        self.assertNotEqual(a, mod.project_id_for(pathlib.Path("/models/other.inp")))

    def test_the_id_is_a_uuid_the_app_accepts(self):
        # The app's validate_id refuses non-UUID project ids on every
        # project-scoped command.
        uuid.UUID(mod.project_id_for(pathlib.Path("/models/city.inp")))


WDS_INP = """[TITLE]
demo
[JUNCTIONS]
;ID  Elev
 J1  10
 J2  12
[RESERVOIRS]
 R1  100
[PIPES]
 P1  R1  J1  100  200  100
 P2  J1  J2  100  200  100
[COORDINATES]
 J1  0  0
"""

UDS_INP = """[JUNCTIONS]
 N1  1  1
[OUTFALLS]
 O1  0  FREE
[CONDUITS]
 C1  N1  O1  100  0.01
[SUBCATCHMENTS]
 S1  G1  N1  5  25  500  0.5
"""


class CountTests(unittest.TestCase):
    # Zero counts in meta.json read as "no network yet" and the app
    # refuses to simulate; the app only refreshes counts on save, which a
    # staged project has never done. Found live: a staged network's Run
    # button refused with exactly that message.

    def test_wds_counts_nodes_and_links(self):
        self.assertEqual(mod.count_elements(WDS_INP, "wds"), (3, 2))

    def test_uds_counts_conveyance_only(self):
        # The subcatchment is not a node or a link; the counts mirror
        # what the app computes from its network on save.
        self.assertEqual(mod.count_elements(UDS_INP, "uds"), (2, 1))

    def test_comments_and_blanks_are_not_elements(self):
        self.assertEqual(mod.count_elements("[JUNCTIONS]\n;comment\n\n", "wds"), (0, 0))

    def test_a_real_network_counts_nonzero(self):
        text = (mod.REPO / "tests" / "benchmarks" / "wds" / "ltown.inp").read_text(errors="replace")
        nodes, links = mod.count_elements(text, "wds")
        self.assertGreater(nodes, 100)
        self.assertGreater(links, 100)


class MetaTests(unittest.TestCase):
    def test_meta_carries_counts_the_run_gate_reads(self):
        self.assertEqual(
            mod.meta_for("City", "wds", 407, 480),
            {"version": 1, "name": "City", "engine": "wds", "nodeCount": 407, "linkCount": 480},
        )

    def test_missing_counts_are_healed_and_real_ones_kept(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            meta = pathlib.Path(d) / "meta.json"
            meta.write_text('{"version": 1, "name": "X", "engine": "wds", "sourceCrs": "EPSG:2229"}')
            self.assertTrue(mod.heal_counts(meta, 3, 2))
            healed = json.loads(meta.read_text())
            self.assertEqual((healed["nodeCount"], healed["linkCount"]), (3, 2))
            # The app's own fields survive the heal.
            self.assertEqual(healed["sourceCrs"], "EPSG:2229")
            # The app's own counts (refreshed on save) are never clobbered.
            self.assertFalse(mod.heal_counts(meta, 999, 999))
            self.assertEqual(json.loads(meta.read_text())["nodeCount"], 3)

    def test_views_mirror_the_frontend(self):
        # Cross-boundary invariant: the TS side asserts the same list in
        # bootOverride.test.ts via PROJECT_VIEWS.
        self.assertEqual(mod.VIEWS, ("overview", "canvas", "editor", "analysis", "report"))


class LaunchEnvTests(unittest.TestCase):
    def test_the_default_leaves_home_alone(self):
        # The real profile is the default because a foreign HOME has no
        # login keychain on macOS: basemap tokens become unreadable and
        # tokened basemaps silently fail (found the hard way).
        with mock.patch.dict("os.environ", {"HOME": "/Users/real"}, clear=True):
            env = mod.launch_env("pid-1", "canvas", None)
        self.assertEqual(env["HOME"], "/Users/real")
        self.assertNotIn("CARGO_HOME", env)

    def test_isolation_moves_home_but_not_the_toolchain(self):
        # Under --isolate, HOME points at the scratch profile while
        # CARGO_HOME/RUSTUP_HOME keep their real locations, or the launch
        # re-downloads the toolchain into the profile.
        with mock.patch.dict("os.environ", {"HOME": "/Users/real"}, clear=True):
            env = mod.launch_env("pid-1", "canvas", pathlib.Path("/scratch"))
        self.assertEqual(env["HOME"], "/scratch")
        self.assertEqual(env["CARGO_HOME"], "/Users/real/.cargo")
        self.assertEqual(env["RUSTUP_HOME"], "/Users/real/.rustup")

    def test_boot_override_names_the_project_and_view(self):
        with mock.patch.dict("os.environ", {"HOME": "/Users/real"}, clear=True):
            env = mod.launch_env("pid-1", "analysis", None)
        self.assertEqual(env["VITE_HYDRA_BOOT_PROJECT"], "pid-1")
        self.assertEqual(env["VITE_HYDRA_BOOT_VIEW"], "analysis")


class MarkerDisciplineTests(unittest.TestCase):
    # In the real profile, staged bundles sit beside the user's own
    # projects; anything destructive must touch only marker-bearing dirs.

    def test_only_marked_bundles_are_listed_as_staged(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            projects = mod.app_projects_dir(pathlib.Path(d))
            mine = projects / "11111111-1111-5111-8111-111111111111"
            theirs = projects / "22222222-2222-4222-8222-222222222222"
            for p in (mine, theirs):
                p.mkdir(parents=True)
                (p / "meta.json").write_text("{}")
            (mine / mod.MARKER).write_text('{"source": "/models/city.inp"}')
            self.assertEqual(mod.staged_bundles(pathlib.Path(d)), [mine])


if __name__ == "__main__":
    unittest.main()
