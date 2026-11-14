"""Pure identity checks for Cargo artifacts and nextest-listed executables."""

from __future__ import annotations

import hashlib
from pathlib import Path
import tempfile
import unittest

from scripts.experiments.common import ExperimentFailure
from scripts.experiments.runner_identity import (
    recheck_binary_hashes,
    verify_nextest_listing,
)


class NextestIdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.target = self.root / "target"
        self.target.mkdir()
        self.unit = self.target / "debug" / "deps" / "unit-test-hash"
        self.unit.parent.mkdir(parents=True)
        self.unit.write_bytes(b"prepared-test-binary")
        self.artifacts = [{
            "package_id": "path+file:///repo/crates/demo#0.1.0",
            "target_name": "unit",
            "executable": str(self.unit),
            "features": ["default", "svg"],
            "profile": {"test": True, "opt_level": "0"},
            "target_kind": ["lib"],
            "sha256": hashlib.sha256(b"prepared-test-binary").hexdigest(),
        }]

    def tearDown(self):
        self.temp.cleanup()

    def listing(self, *, target_dir=None, binary_name="unit", binary_path=None, testcases=None):
        suite = {
            "package-id": self.artifacts[0]["package_id"],
            "binary-id": "demo::unit",
            "binary-name": binary_name,
            "binary-path": str(binary_path or self.unit),
            "testcases": testcases if testcases is not None else {
                "tests::same_case": {"kind": "test", "ignored": False,
                                     "filter-match": {"status": "matches"}},
            },
        }
        return {
            "rust-build-meta": {"target-directory": str(target_dir or self.target)},
            "rust-suites": {"demo::unit": suite},
        }

    def test_same_test_ids_from_a_different_binary_are_rejected(self):
        alternate = self.target / "debug" / "deps" / "unit-other-hash"
        alternate.write_bytes(b"different-test-binary")
        # Package, target name and test name still form the same test ID.
        listed = self.listing(binary_path=alternate)
        testcase_name = next(iter(listed["rust-suites"]["demo::unit"]["testcases"]))
        cargo_id = f"demo::{self.artifacts[0]['target_name']}::{testcase_name}"
        nextest_id = f"demo::{listed['rust-suites']['demo::unit']['binary-name']}::{testcase_name}"
        self.assertEqual(cargo_id, nextest_id)
        with self.assertRaisesRegex(ExperimentFailure, "does not match a prepared Cargo artifact"):
            verify_nextest_listing(
                listed, self.artifacts, self.target,
                exact_suite_set=True,
            )

    def test_same_test_ids_with_changed_prepared_binary_hash_are_rejected(self):
        listed = self.listing()
        self.unit.write_bytes(b"rebuilt-with-different-feature-or-profile")
        testcase_name = next(iter(listed["rust-suites"]["demo::unit"]["testcases"]))
        cargo_case_id = f"demo::{self.artifacts[0]['target_name']}::{testcase_name}"
        nextest_case_id = f"demo::{listed['rust-suites']['demo::unit']['binary-name']}::{testcase_name}"
        case_id_before = cargo_case_id
        case_id_after = nextest_case_id
        self.assertEqual(case_id_before, case_id_after)
        with self.assertRaisesRegex(ExperimentFailure, "changed before nextest listing"):
            verify_nextest_listing(listed, self.artifacts, self.target, exact_suite_set=True)

    def test_full_identity_includes_zero_case_suites_and_cargo_build_metadata(self):
        empty = self.target / "debug" / "deps" / "empty-test-hash"
        empty.write_bytes(b"empty-harness")
        second_artifact = {
            **self.artifacts[0], "target_name": "empty", "executable": str(empty),
            "features": [], "profile": {"test": True}, "target_kind": ["test"],
            "sha256": hashlib.sha256(b"empty-harness").hexdigest(),
        }
        data = self.listing()
        data["rust-suites"]["demo::empty"] = {
            "package-id": second_artifact["package_id"], "binary-id": "demo::empty",
            "binary-name": "empty", "binary-path": str(empty), "testcases": {},
        }
        identity = verify_nextest_listing(
            data, [*self.artifacts, second_artifact], self.target, exact_suite_set=True,
        )
        self.assertTrue(identity["verified"])
        self.assertEqual(identity["listed_suite_count"], 2)
        self.assertEqual(identity["prepared_artifact_count"], 2)
        empty_record = next(row for row in identity["binaries"] if row["binary_name"] == "empty")
        self.assertEqual(empty_record["listed_testcase_count"], 0)
        unit_record = next(row for row in identity["binaries"] if row["binary_name"] == "unit")
        self.assertEqual(unit_record["cargo_features"], ["default", "svg"])
        self.assertEqual(unit_record["cargo_profile"], {"test": True, "opt_level": "0"})

    def test_target_directory_mismatch_is_rejected(self):
        other = self.root / "other-target"
        other.mkdir()
        with self.assertRaisesRegex(ExperimentFailure, "target directory differs"):
            verify_nextest_listing(self.listing(target_dir=other), self.artifacts,
                                   self.target, exact_suite_set=True)

    def test_binary_hash_mutation_after_listing_is_rejected(self):
        identity = verify_nextest_listing(self.listing(), self.artifacts,
                                          self.target, exact_suite_set=True)
        recheck_binary_hashes(identity, self.target)
        self.unit.write_bytes(b"mutated-by-nextest-build")
        with self.assertRaisesRegex(ExperimentFailure, "changed after identity proof"):
            recheck_binary_hashes(identity, self.target)

    def test_binary_name_mismatch_is_rejected(self):
        with self.assertRaisesRegex(ExperimentFailure, "does not match a prepared Cargo artifact"):
            verify_nextest_listing(self.listing(binary_name="other"), self.artifacts,
                                   self.target, exact_suite_set=True)

    def test_exact_full_inventory_rejects_an_omitted_zero_case_binary(self):
        empty = self.target / "debug" / "deps" / "empty-test-hash"
        empty.write_bytes(b"empty-harness")
        second_artifact = {**self.artifacts[0], "target_name": "empty", "executable": str(empty),
                           "sha256": hashlib.sha256(b"empty-harness").hexdigest()}
        with self.assertRaisesRegex(ExperimentFailure, "inventory differs"):
            verify_nextest_listing(self.listing(), [*self.artifacts, second_artifact],
                                   self.target, exact_suite_set=True)


if __name__ == "__main__":
    unittest.main()
