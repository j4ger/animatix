#!/usr/bin/env python3
"""Observe a command without changing its arguments or capturing its output.

Each invocation writes a separate best-effort artifact. No command, arguments,
working directory or unvalidated environment value is included in that artifact.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid


LABEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,79}\Z")


def safe_environment(environ=None):
    """Return only bounded, recognized Cargo tuning values, never arbitrary env."""
    environ = os.environ if environ is None else environ
    validators = {
        "CARGO_BUILD_JOBS": lambda v: bool(re.fullmatch(r"[1-9][0-9]{0,3}", v)),
        "CARGO_INCREMENTAL": lambda v: v in {"0", "1"},
    }
    for profile in ("DEV", "TEST", "RELEASE", "BENCH"):
        prefix = f"CARGO_PROFILE_{profile}_"
        validators.update({
            prefix + "DEBUG": lambda v: v in {
                "0", "1", "2", "true", "false", "none", "limited", "full",
                "line-directives-only", "line-tables-only",
            },
            prefix + "OPT_LEVEL": lambda v: v in {"0", "1", "2", "3", "s", "z"},
            prefix + "INCREMENTAL": lambda v: v in {"true", "false"},
            prefix + "CODEGEN_UNITS": lambda v: bool(re.fullmatch(r"[1-9][0-9]{0,3}", v)),
            prefix + "LTO": lambda v: v in {"true", "false", "off", "thin", "fat"},
        })
    return {
        key: value for key, validate in validators.items()
        if isinstance(value := environ.get(key), str) and validate(value)
    }


def record(data):
    """Exclusive creation avoids collisions and shared-file races."""
    try:
        directory = Path(os.environ.get("ANIMATIX_PHASE_TIMINGS_DIR", "target/ci/phases"))
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / (uuid.uuid4().hex + ".json")
        with path.open("x", encoding="utf-8") as stream:
            json.dump(data, stream, sort_keys=True)
            stream.write("\n")
    except (OSError, ValueError):
        print("warning: phase timing artifact could not be recorded", file=sys.stderr)


def label(value):
    if not LABEL.fullmatch(value):
        raise argparse.ArgumentTypeError("expected a short alphanumeric phase label")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gate", required=True, type=label)
    parser.add_argument("--phase", required=True, type=label)
    parser.add_argument("--kind", required=True, choices=("cargo", "execution", "mixed"))
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("a command is required after --")
    started_at = datetime.now(timezone.utc).isoformat()
    start = time.monotonic()
    child = None
    previous = {}
    pending_signals = set()

    def forward(signum, _frame):
        if child is None:
            pending_signals.add(signum)
        else:
            try:
                if os.name == "posix":
                    os.killpg(child.pid, signum)
                else:
                    child.send_signal(signum)
            except ProcessLookupError:
                pass

    # Keep the wrapper alive long enough to record termination, and pass normal
    # cancellation on to the child. The child's result remains authoritative.
    for signum in (signal.SIGINT, signal.SIGTERM):
        previous[signum] = signal.signal(signum, forward)
    try:
        try:
            # POSIX: isolate the command tree so runner group cancellation reaches
            # the wrapper once, then is forwarded once to every descendant.
            # Windows currently falls back to direct-child forwarding only.
            child = subprocess.Popen(command, start_new_session=(os.name == "posix"))
            for signum in tuple(pending_signals):
                forward(signum, None)
            result = child.wait()
        except FileNotFoundError:
            print("phase timing: command not found", file=sys.stderr)
            result = 127
        except OSError:
            print("phase timing: command could not be started", file=sys.stderr)
            result = 126
        record({
            "schema": 1,
            "gate": args.gate,
            "phase": args.phase,
            "kind": args.kind,
            "started_at": started_at,
            "finished_at": datetime.now(timezone.utc).isoformat(),
            "duration_seconds": time.monotonic() - start,
            "exit_code": result if result >= 0 else None,
            "signal": -result if result < 0 else None,
            "environment": safe_environment(),
        })
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)
    if result < 0:
        signum = -result
        if signum not in (signal.SIGKILL, signal.SIGSTOP):
            signal.signal(signum, signal.SIG_DFL)
        os.kill(os.getpid(), signum)
        return 128 + signum  # Fallback if a platform delays signal delivery.
    return result


if __name__ == "__main__":
    sys.exit(main())
