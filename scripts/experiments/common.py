"""Shared safe output ownership, locking, timing, and process-tree RSS helpers."""

from __future__ import annotations

from contextlib import contextmanager, nullcontext
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import threading
import time
from typing import Mapping, Sequence


MARKER_NAME = ".animatix-experiment-owner.json"
TARGET_MARKER_NAME = ".animatix-owned-target.json"


class ExperimentFailure(RuntimeError):
    """An experiment setup or measured command failed."""


def host_snapshot() -> dict:
    """Record the host and image for comparisons on the same measured machine."""
    cpu = platform.processor()
    if Path("/proc/cpuinfo").is_file():
        cpu = next((line.split(":", 1)[1].strip()
                    for line in Path("/proc/cpuinfo").read_text().splitlines()
                    if line.startswith("model name")), cpu)
    try:
        memory = os.sysconf("SC_PHYS_PAGES") * os.sysconf("SC_PAGE_SIZE")
    except (ValueError, OSError):
        probe = subprocess.run(["sysctl", "-n", "hw.memsize"], capture_output=True, text=True)
        memory = int(probe.stdout.strip()) if probe.returncode == 0 else None
    return {
        "uname": list(platform.uname()), "cpu_model": cpu,
        "cpu_count": os.cpu_count(),
        "cpu_affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "load_average": list(os.getloadavg()),
        "physical_memory_bytes": memory,
        "runner_image": {key: os.environ[key] for key in
                         ("ImageOS", "ImageVersion", "RUNNER_OS", "RUNNER_ARCH", "RUNNER_NAME")
                         if key in os.environ},
    }


class ExperimentOutput:
    """An output tree that may only reset directories it explicitly marked."""

    def __init__(self, repo: Path, output: Path, experiment: str, metadata: dict):
        self.repo = Path(repo).resolve()
        requested = Path(output)
        self.directory = (self.repo / requested).resolve() if not requested.is_absolute() else requested.resolve()
        self.experiment = experiment
        self.metadata = dict(metadata)
        self.directory.mkdir(parents=True, exist_ok=True)
        marker = self.directory / MARKER_NAME
        expected = {"schema": 1, "repo": str(self.repo), "experiment": self.experiment}
        if marker.exists():
            try:
                existing = json.loads(marker.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as error:
                raise ExperimentFailure(f"invalid experiment ownership marker: {marker}: {error}") from error
            if existing != expected:
                raise ExperimentFailure(f"output directory is owned by a different experiment: {self.directory}")
        elif any(self.directory.iterdir()):
            raise ExperimentFailure(f"refusing unmarked nonempty experiment output: {self.directory}")
        else:
            self._write_json(marker, expected)
        if self.records_path.exists() and self.records_path.stat().st_size:
            raise ExperimentFailure(
                f"output already contains a recorded run; choose a fresh namespace: {self.directory}"
            )
        self.logs_dir.mkdir(parents=True, exist_ok=True)

    @property
    def logs_dir(self) -> Path:
        return self.directory / "logs"

    @property
    def metadata_path(self) -> Path:
        return self.directory / "metadata.json"

    @property
    def records_path(self) -> Path:
        return self.directory / "records.jsonl"

    @staticmethod
    def _write_json(path: Path, value: dict):
        temporary = path.with_suffix(path.suffix + ".tmp")
        temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        temporary.replace(path)

    def write_metadata(self):
        self._write_json(self.metadata_path, self.metadata)

    def write_json(self, relative_path: str | Path, value: dict):
        relative = Path(relative_path)
        if relative.is_absolute() or not relative.parts or any(part in {"", ".", ".."} for part in relative.parts):
            raise ExperimentFailure(f"unsafe experiment artifact path: {relative_path!r}")
        path = (self.directory / relative).resolve()
        if not path.is_relative_to(self.directory) or path in {self.metadata_path, self.records_path}:
            raise ExperimentFailure(f"experiment artifact escapes or aliases reserved output: {relative_path!r}")
        path.parent.mkdir(parents=True, exist_ok=True)
        self._write_json(path, value)

    def append_record(self, record: dict):
        with self.records_path.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(record, sort_keys=True) + "\n")
            stream.flush()
            os.fsync(stream.fileno())

    def owned_target(self, relative_path: str | Path, reset: bool = False) -> Path:
        """Create an explicitly marked child directory; reset it only if its marker matches."""
        relative = Path(relative_path)
        if relative.is_absolute() or not relative.parts or any(part in {"", ".", ".."} for part in relative.parts):
            raise ExperimentFailure(f"unsafe owned target path: {relative_path!r}")
        target = (self.directory / relative).resolve()
        if not target.is_relative_to(self.directory) or target == self.directory:
            raise ExperimentFailure(f"owned target escapes experiment output: {relative_path!r}")
        marker = target / TARGET_MARKER_NAME
        expected = {
            "schema": 1, "repo": str(self.repo), "experiment": self.experiment,
            "relative_path": relative.as_posix(),
        }
        if target.exists():
            if marker.exists():
                try:
                    existing = json.loads(marker.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError) as error:
                    raise ExperimentFailure(f"invalid target ownership marker: {marker}: {error}") from error
                if existing != expected:
                    raise ExperimentFailure(f"target is owned by another experiment: {target}")
                if reset:
                    shutil.rmtree(target)
            elif any(target.iterdir()):
                raise ExperimentFailure(f"refusing unmarked nonempty target directory: {target}")
            elif reset:
                target.rmdir()
        target.mkdir(parents=True, exist_ok=True)
        if not marker.exists():
            self._write_json(marker, expected)
        return target

    @contextmanager
    def machine_lock(self):
        """Serialize costly measurements that share one host or workspace."""
        # Nix gives each shell its own TMPDIR; measurements still share a host.
        lock_path = Path("/tmp") / f"animatix-experiment-machine-{os.getuid()}.lock"
        with lock_path.open("a+") as lock_file:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)


def _proc_tree_rss_linux(root_pid: int) -> int:
    parents: dict[int, list[int]] = {}
    rss_by_pid: dict[int, int] = {}
    try:
        page_size = os.sysconf("SC_PAGE_SIZE")
        for entry in Path("/proc").iterdir():
            if not entry.name.isdecimal():
                continue
            pid = int(entry.name)
            try:
                data = (entry / "stat").read_text(encoding="ascii")
                fields = data[data.rfind(")") + 2:].split()
                parent = int(fields[1])
                rss = max(0, int(fields[21])) * page_size
            except (OSError, ValueError, IndexError):
                continue
            parents.setdefault(parent, []).append(pid)
            rss_by_pid[pid] = rss
    except OSError:
        return 0
    total = 0
    stack = [root_pid]
    seen = set()
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        total += rss_by_pid.get(pid, 0)
        stack.extend(parents.get(pid, ()))
    return total


def _proc_tree_rss_ps(root_pid: int) -> int:
    try:
        result = subprocess.run(
            ["ps", "-axo", "pid=,ppid=,rss="], capture_output=True, text=True,
            check=False, timeout=2,
        )
    except (OSError, subprocess.TimeoutExpired):
        return 0
    if result.returncode:
        return 0
    parents: dict[int, list[int]] = {}
    rss_by_pid: dict[int, int] = {}
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) != 3:
            continue
        try:
            pid, parent, rss_kib = map(int, fields)
        except ValueError:
            continue
        parents.setdefault(parent, []).append(pid)
        rss_by_pid[pid] = rss_kib * 1024
    total = 0
    stack = [root_pid]
    seen = set()
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        total += rss_by_pid.get(pid, 0)
        stack.extend(parents.get(pid, ()))
    return total


def process_tree_rss_bytes(root_pid: int) -> int:
    return _proc_tree_rss_linux(root_pid) if Path("/proc").exists() else _proc_tree_rss_ps(root_pid)


def source_fingerprint(repo: Path) -> dict:
    """Hash the checkout identity plus tracked diffs and nonignored untracked files."""
    repo = Path(repo).resolve()
    head = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True,
        text=True, check=False,
    )
    if head.returncode:
        raise ExperimentFailure(f"cannot identify source revision in {repo}")
    diff = subprocess.run(
        ["git", "-C", str(repo), "diff", "--binary", "HEAD", "--"],
        capture_output=True, check=False,
    )
    if diff.returncode:
        raise ExperimentFailure(f"cannot fingerprint tracked source changes in {repo}")
    untracked = subprocess.run(
        ["git", "-C", str(repo), "ls-files", "--others", "--exclude-standard", "-z"],
        capture_output=True, check=False,
    )
    if untracked.returncode:
        raise ExperimentFailure(f"cannot enumerate untracked source files in {repo}")
    digest = hashlib.sha256()
    digest.update(head.stdout.strip().encode())
    digest.update(diff.stdout)
    names = [name for name in untracked.stdout.split(b"\0") if name]
    for name in sorted(names):
        path = repo / os.fsdecode(name)
        digest.update(b"\0" + name + b"\0")
        try:
            if path.is_file():
                with path.open("rb") as source:
                    for block in iter(lambda: source.read(1024 * 1024), b""):
                        digest.update(block)
            elif path.is_symlink():
                digest.update(os.readlink(path).encode("utf-8", "surrogateescape"))
        except OSError as error:
            raise ExperimentFailure(f"cannot fingerprint untracked source {path}: {error}") from error
    return {
        "head": head.stdout.strip(),
        "tracked_diff_sha256": hashlib.sha256(diff.stdout).hexdigest(),
        "untracked_file_count": len(names),
        "workspace_sha256": digest.hexdigest(),
    }


def run_timed(name: str, command: Sequence[str], cwd: Path | str,
              env: Mapping[str, str | None] | None, log_path: Path | str,
              timeout_seconds: float | None = None,
              stderr_path: Path | str | None = None) -> dict:
    """Run one measurement and sample child RSS.

    With ``stderr_path``, ``log_path`` contains stdout and diagnostics are kept
    separately; otherwise both streams share ``log_path``.
    """
    command = [str(part) for part in command]
    cwd = Path(cwd).resolve()
    log_path = Path(log_path).resolve()
    log_path.parent.mkdir(parents=True, exist_ok=True)
    stderr_log_path = Path(stderr_path).resolve() if stderr_path is not None else None
    if stderr_log_path == log_path:
        raise ValueError("stdout and stderr log paths must differ")
    if stderr_log_path is not None:
        stderr_log_path.parent.mkdir(parents=True, exist_ok=True)
    child_env = os.environ.copy()
    for key, value in (env or {}).items():
        if value is None:
            child_env.pop(key, None)
        else:
            child_env[key] = str(value)

    started = time.monotonic()
    peak = 0
    timed_out = False
    error = None
    process = None
    stderr_context = stderr_log_path.open("wb") if stderr_log_path is not None else nullcontext(None)
    with log_path.open("wb") as log_file, stderr_context as stderr_file:
        try:
            process = subprocess.Popen(
                command, cwd=cwd, env=child_env, stdout=log_file,
                stderr=stderr_file if stderr_log_path is not None else subprocess.STDOUT,
                start_new_session=True,
            )
            while process.poll() is None:
                peak = max(peak, process_tree_rss_bytes(process.pid))
                if timeout_seconds is not None and time.monotonic() - started > timeout_seconds:
                    timed_out = True
                    try:
                        os.killpg(process.pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass
                    break
                time.sleep(0.1)
            process.wait()
            peak = max(peak, process_tree_rss_bytes(process.pid))
            status = process.returncode
            if status < 0:
                status = 128 - status
        except OSError as exception:
            error = str(exception)
            log_file.write((error + "\n").encode("utf-8", "replace"))
            status = 127
        finally:
            log_file.flush()
            os.fsync(log_file.fileno())
    elapsed = time.monotonic() - started
    return {
        "name": name,
        "command": command,
        "cwd": str(cwd),
        "log_path": str(log_path),
        "stderr_log_path": str(stderr_log_path) if stderr_log_path is not None else None,
        "elapsed_seconds": elapsed,
        "exit_code": status,
        "timed_out": timed_out,
        "peak_tree_rss_bytes": peak,
        "rss_sampling_interval_seconds": 0.1,
        "error": error,
    }
