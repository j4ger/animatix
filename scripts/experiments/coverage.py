"""Pure test inventory and executed-case coverage parsing for runner experiments."""

from __future__ import annotations

import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

from scripts.experiments.common import ExperimentFailure


CASE_ALIAS = {
    "move_requires_to_vec2_modifier": "motion_actions_reject_invalid_modifier_values",
    "rotate_requires_numeric_by_modifier": "motion_actions_reject_invalid_modifier_values",
    "scale_requires_positive_numeric_by_modifier": "motion_actions_reject_invalid_modifier_values",
    "shift_requires_by_vec2_modifier": "motion_actions_reject_invalid_modifier_values",
}
TEST_LIST_RE = re.compile(r"^(.*?)\s*:\s*test\s*$")
DOC_RESULT_RE = re.compile(r"^test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)(?:\s+\(.*\))?\s*$")
LIBTEST_RESULT_RE = re.compile(r"^test\s+(.+?)\s+\.\.\.\s+(ok|FAILED|ignored)\s*$")


def parse_artifacts(path: Path, workspace_members: set[str], target_dir: Path | None = None) -> list[dict]:
    """Read Cargo JSON and keep executable workspace test artifacts."""
    artifacts = []
    seen = set()
    for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(record, dict) or record.get("reason") != "compiler-artifact" or not record.get("executable"):
            continue
        if record.get("package_id") not in workspace_members:
            continue
        target = record.get("target", {})
        profile = record.get("profile", {})
        if not target.get("test") or not profile.get("test"):
            continue
        executable = Path(record["executable"]).resolve()
        if not executable.is_file():
            continue
        if target_dir is not None and not executable.is_relative_to(Path(target_dir).resolve()):
            raise ExperimentFailure(f"test executable is outside the selected target directory: {executable}")
        key = (record["package_id"], target.get("name"), str(executable))
        if key in seen:
            continue
        seen.add(key)
        artifacts.append({
            "package_id": record["package_id"], "package_name": None,
            "target_name": target.get("name", executable.name),
            "features": record.get("features", []),
            "profile": profile,
            "target_kind": target.get("kind", []),
            "manifest_path": record.get("manifest_path"),
            "executable": str(executable),
        })
    if not artifacts:
        raise ExperimentFailure(f"no executable workspace test artifacts found in {path}")
    return artifacts


def parse_test_list(text: str) -> list[str]:
    names = []
    for line in text.splitlines():
        match = TEST_LIST_RE.match(line.strip())
        if match:
            names.append(match.group(1))
    return names


def parse_nextest_listing(data: dict) -> tuple[list[dict], int]:
    """Parse nextest list JSON; rust-suites/testcases are keyed objects."""
    suites = data.get("rust-suites") if isinstance(data, dict) else None
    if isinstance(suites, dict):
        suite_rows = [(binary_id, suite) for binary_id, suite in suites.items()]
    elif isinstance(suites, list):
        suite_rows = [(suite.get("binary-id"), suite) for suite in suites]
    else:
        raise ExperimentFailure("nextest JSON has no rust-suites object or list")
    cases = []
    for binary_id, suite in suite_rows:
        package = suite.get("package-name")
        binary = suite.get("binary-name")
        testcases = suite.get("testcases", {})
        if isinstance(testcases, dict):
            testcase_rows = testcases.items()
        elif isinstance(testcases, list):
            testcase_rows = ((testcase.get("name"), testcase) for testcase in testcases)
        else:
            raise ExperimentFailure(f"nextest testcases for {binary_id} are not an object or list")
        for name, testcase in testcase_rows:
            status = testcase.get("filter-match", {}).get("status", "matches")
            if status != "matches":
                continue
            ignored = bool(testcase.get("ignored", False))
            name = testcase.get("name") or name
            if not name:
                continue
            cases.append({"id": f"{package}::{binary}::{name}", "package": package,
                          "binary": binary, "name": name, "ignored": ignored,
                          "binary_id": suite.get("binary-id") or binary_id})
    ids = [case["id"] for case in cases]
    if len(ids) != len(set(ids)):
        raise ExperimentFailure("nextest list contains duplicate canonical test IDs")
    return cases, len(suite_rows)


def parse_nextest_junit_ids(report_path: Path, cases: list[dict]) -> tuple[list[str], list[dict]]:
    if not report_path.is_file():
        raise ExperimentFailure(f"nextest did not produce the required JUnit report: {report_path}")
    try:
        root = ET.parse(report_path).getroot()
    except ET.ParseError as error:
        raise ExperimentFailure(f"nextest JUnit report is invalid: {error}") from error
    suites = [root] if root.tag == "testsuite" else list(root.findall("testsuite"))
    by_binary: dict[str, list[dict]] = {}
    for case in cases:
        by_binary.setdefault(case["binary_id"], []).append(case)
    actual = []
    unresolved = []
    for suite in suites:
        suite_name = suite.attrib.get("name", "")
        candidates = by_binary.get(suite_name, [])
        if not candidates:
            candidates = [case for binary_id, suite_cases in by_binary.items()
                          if suite_name.endswith(binary_id) for case in suite_cases]
        for testcase in suite.findall("testcase"):
            if testcase.find("skipped") is not None:
                continue
            name = testcase.attrib.get("name", "")
            matching = [case for case in candidates if case["name"] == name]
            if len(matching) != 1:
                unresolved.append({"suite": suite_name, "name": name,
                                   "candidate_count": len(matching)})
                continue
            actual.append(matching[0]["id"])
    if unresolved:
        raise ExperimentFailure(f"cannot map nextest JUnit test IDs: {unresolved[:8]}")
    return sorted(set(actual)), [{"suite": suite.attrib.get("name"),
                                  "tests": len(suite.findall("testcase"))} for suite in suites]


def canonical_ids(cases: list[dict], active_only: bool = True) -> set[str]:
    """Return exact IDs for the current source; historical aliases are opt-in."""
    return {case["id"] for case in cases if not (active_only and case.get("ignored", False))}


def historical_canonical_ids(cases: list[dict]) -> set[str]:
    result = set()
    for case in cases:
        test_id = case["id"]
        for old, new in CASE_ALIAS.items():
            if test_id.endswith("::" + old):
                test_id = test_id[: -len(old)] + new
                break
        result.add(test_id)
    return result


def ignored_ids(cases: list[dict]) -> set[str]:
    return canonical_ids([case for case in cases if case.get("ignored", False)], active_only=False)


def coverage_diff(expected: set[str], actual: set[str]) -> dict:
    return {"expected_count": len(expected), "actual_count": len(actual),
            "missing": sorted(expected - actual), "unexpected": sorted(actual - expected),
            "equal": expected == actual}


def parse_libtest_results(log_path: Path) -> list[dict]:
    results = []
    for line in Path(log_path).read_text(encoding="utf-8", errors="replace").splitlines():
        match = LIBTEST_RESULT_RE.match(line.strip())
        if match:
            results.append({"name": match.group(1), "status": match.group(2)})
    return results


def parse_doc_ids(records: list[dict]) -> list[str]:
    ids = []
    for record in records:
        path = record.get("log_path")
        if not path or not Path(path).exists():
            continue
        in_doc_tests = False
        for line in Path(path).read_text(encoding="utf-8", errors="replace").splitlines():
            if re.match(r"^\s*Doc-tests\s+", line):
                in_doc_tests = True
                continue
            if in_doc_tests and re.match(r"^\s*Running\s+", line):
                in_doc_tests = False
            if not in_doc_tests:
                continue
            match = DOC_RESULT_RE.match(line.strip())
            if match:
                ids.append(match.group(1))
    return sorted(ids)


def parse_cargo_executed_ids(log_path: Path, harnesses: list[dict]) -> tuple[list[str], list[dict]]:
    by_path = {str(Path(item["executable"]).resolve()): item for item in harnesses}
    by_basename: dict[str, list[dict]] = {}
    for item in harnesses:
        by_basename.setdefault(Path(item["executable"]).name, []).append(item)
    current = None
    observed_ids = []
    unresolved = []
    for line in Path(log_path).read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if stripped.startswith("Doc-tests "):
            current = None
            continue
        if stripped.startswith("Running ") and "(" in stripped and stripped.endswith(")"):
            executable_text = stripped[stripped.rfind("(") + 1:-1]
            executable = str(Path(executable_text).resolve())
            current = by_path.get(executable)
            if current is None:
                candidates = by_basename.get(Path(executable_text).name, [])
                current = candidates[0] if len(candidates) == 1 else None
            if current is None:
                unresolved.append({"heading": stripped, "executable": executable_text})
            continue
        match = LIBTEST_RESULT_RE.match(stripped)
        if match and current is not None and match.group(2) in {"ok", "FAILED"}:
            case = next((case for case in current["cases"] if case["name"] == match.group(1)), None)
            if case is not None:
                observed_ids.append(case["id"])
    return sorted(set(observed_ids)), unresolved
