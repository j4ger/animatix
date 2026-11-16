#!/usr/bin/env python3
"""Report source-size signals; runtime test inventories remain the coverage oracle."""

from collections import defaultdict
import json
from pathlib import Path
import subprocess
import sys


def inventory(root):
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=root)
    packages = defaultdict(lambda: defaultdict(int))
    for name in sorted(set(path.decode() for path in paths.split(b"\0") if path)):
        path = Path(name)
        if path.suffix != ".rs" or not (path.parts[0] in {"crates", "tools"} or name.startswith("web/tools/")):
            continue
        if not (root / path).is_file():
            continue
        lines = (root / path).read_text().splitlines()
        owner_depth = 3 if name.startswith("web/tools/") else 2
        counts = packages["/".join(path.parts[:owner_depth])]
        counts["files"] += 1
        counts["lines"] += len(lines)
        counts["blank_lines"] += sum(not line.strip() for line in lines)
        counts["standalone_comment_lines"] += sum(line.lstrip().startswith("//") for line in lines)
        counts["test_attributes"] += sum(line.strip() == "#[test]" for line in lines)
        counts["cfg_test_markers"] += sum(line.strip() == "#[cfg(test)]" for line in lines)
        if "tests" in path.parts or path.stem == "tests" or path.stem.endswith("_tests"):
            counts["test_path_lines"] += len(lines)
    return {"method": "tracked and non-ignored workspace Rust; line/comment/test-attribute signals overlap",
            "coverage_warning": "attributes and test paths are not executable case counts",
            "packages": dict(sorted(packages.items()))}


if __name__ == "__main__":
    repository = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[2]
    print(json.dumps(inventory(repository), indent=2, sort_keys=True))
