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
LIBTEST_START_RE = re.compile(r"^running\s+(\d+)\s+tests?\s*$")
LIBTEST_SUMMARY_RE = re.compile(r"^test result:")


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
        stderr_path = record.get("stderr_log_path")
        if stderr_path:
            # Dispatch records name a package/binary directly; their stderr is
            # from the test process, not Cargo's `Doc-tests` stream.
            if all(key in record for key in ("package", "binary", "filter")):
                continue
            sections = _parse_cargo_stream_sections(Path(path), Path(stderr_path))
            for block in sections["doc_blocks"]:
                ids.extend(row["name"] for row in block["results"])
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


def _parse_libtest_blocks(stdout_path: Path) -> list[dict]:
    blocks = []
    current = None
    for line in Path(stdout_path).read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        start = LIBTEST_START_RE.match(stripped)
        if start:
            if current is not None:
                raise ExperimentFailure("Cargo stdout started a new test block before the previous result")
            current = {"test_count": int(start.group(1)), "results": []}
            continue
        if current is None:
            continue
        if LIBTEST_SUMMARY_RE.match(stripped):
            if len(current["results"]) != current["test_count"]:
                raise ExperimentFailure(
                    "Cargo stdout test block listed "
                    f"{len(current['results'])} results but announced {current['test_count']}"
                )
            blocks.append(current)
            current = None
            continue
        match = LIBTEST_RESULT_RE.match(stripped)
        if match:
            current["results"].append({"name": match.group(1), "status": match.group(2)})
    if current is not None:
        raise ExperimentFailure("Cargo stdout ended before a test-result summary")
    return blocks


def _parse_cargo_stderr_headings(stderr_path: Path) -> list[dict]:
    headings = []
    for line in Path(stderr_path).read_text(encoding="utf-8", errors="replace").splitlines():
        stripped = line.strip()
        if stripped.startswith("Doc-tests "):
            headings.append({"kind": "doc-tests", "heading": stripped})
            continue
        if stripped.startswith("Running "):
            executable = (stripped[stripped.rfind("(") + 1:-1]
                          if "(" in stripped and stripped.endswith(")") else None)
            headings.append({"kind": "harness", "heading": stripped, "executable": executable})
    return headings


def _parse_cargo_stream_sections(stdout_path: Path, stderr_path: Path) -> dict:
    """Pair Cargo unit headings and separate rustdoc output blocks by phase."""
    headings = _parse_cargo_stderr_headings(stderr_path)
    blocks = _parse_libtest_blocks(stdout_path)
    doc_start = next((index for index, heading in enumerate(headings)
                      if heading["kind"] == "doc-tests"), len(headings))
    unit_headings = headings[:doc_start]
    doc_headings = headings[doc_start:]
    if any(heading["kind"] != "doc-tests" for heading in doc_headings):
        raise ExperimentFailure("Cargo emitted a test-harness heading after the doc-test phase began")
    if len(blocks) < len(unit_headings):
        raise ExperimentFailure(
            "Cargo stdout has fewer complete test blocks than unit-test headings: "
            f"{len(blocks)} blocks for {len(unit_headings)} harnesses"
        )
    unit_blocks = blocks[:len(unit_headings)]
    doc_blocks = blocks[len(unit_headings):]
    if doc_blocks and not doc_headings:
        raise ExperimentFailure("Cargo stdout contains doc-test blocks without a Doc-tests heading")
    return {
        "unit_sections": list(zip(unit_headings, unit_blocks)),
        "doc_headings": doc_headings,
        "doc_blocks": doc_blocks,
    }


def _harness_for_cargo_heading(heading: dict, by_path: dict, by_basename: dict) -> dict | None:
    executable_text = heading["executable"]
    if not executable_text:
        return None
    executable = str(Path(executable_text).resolve())
    current = by_path.get(executable)
    if current is not None:
        return current
    candidates = by_basename.get(Path(executable_text).name, [])
    return candidates[0] if len(candidates) == 1 else None


def parse_cargo_executed_ids(log_path: Path, harnesses: list[dict],
                             stderr_path: Path | None = None) -> tuple[list[str], list[dict]]:
    if stderr_path is not None:
        by_path = {str(Path(item["executable"]).resolve()): item for item in harnesses}
        by_basename: dict[str, list[dict]] = {}
        for item in harnesses:
            by_basename.setdefault(Path(item["executable"]).name, []).append(item)
        observed_ids = []
        sections = _parse_cargo_stream_sections(log_path, stderr_path)
        for heading, block in sections["unit_sections"]:
            current = _harness_for_cargo_heading(heading, by_path, by_basename)
            if current is None:
                raise ExperimentFailure(f"Cargo test heading does not identify a listed harness: {heading['heading']}")
            for row in block["results"]:
                if row["status"] not in {"ok", "FAILED"}:
                    continue
                matches = [case for case in current["cases"] if case["name"] == row["name"]]
                if len(matches) != 1:
                    raise ExperimentFailure(
                        f"Cargo result `{row['name']}` matched {len(matches)} listed cases in {heading['heading']}"
                    )
                observed_ids.append(matches[0]["id"])
        return sorted(set(observed_ids)), []

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
