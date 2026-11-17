#!/usr/bin/env python3
"""Linux-only bounded diagnostic subprocesses; never report a guard as success."""
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import time


def group_rss_kib(group):
    total = 0
    for entry in Path('/proc').iterdir():
        if not entry.name.isdecimal():
            continue
        try:
            stat = (entry / 'stat').read_text()
            if int(stat[stat.rfind(')') + 2:].split()[2]) != group:
                continue
            for line in (entry / 'status').read_text().splitlines():
                if line.startswith('VmRSS:'):
                    total += int(line.split()[1])
        except (OSError, ValueError, IndexError):
            continue  # process exited during observation
    return total


def stop_group(child, signum=signal.SIGTERM):
    try:
        os.killpg(child.pid, signum)
    except ProcessLookupError:
        pass


def run(label, command, evidence, rss_mib=10240, seconds=1800, env=None):
    evidence = Path(evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    free = shutil.disk_usage('.').free
    if free < 3 * 1024**3:
        raise RuntimeError('less than 3 GiB free before command')
    child = None
    reason = None
    peak = 0
    min_available = None
    old = {}
    cancelled = []
    log = evidence / f'{label}.log'

    def cancel(signum, _frame):
        cancelled.append(signum)
        if child is not None:
            stop_group(child, signum)

    for sig in (signal.SIGTERM, signal.SIGINT):
        old[sig] = signal.signal(sig, cancel)
    try:
        with log.open('wb') as stream:
            child = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT,
                                     env=env, start_new_session=True)
            while child.poll() is None:
                mem = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines())
                available = int(mem['MemAvailable'].split()[0])
                min_available = available if min_available is None else min(min_available, available)
                observed = group_rss_kib(child.pid)
                peak = max(peak, observed)
                free = min(free, shutil.disk_usage('.').free)
                if cancelled:
                    reason = 'cancelled'
                elif observed > rss_mib * 1024:
                    reason = 'rss_guard'
                elif available < 1536 * 1024:
                    reason = 'available_memory_guard'
                elif free < 2 * 1024**3:
                    reason = 'disk_guard'
                elif time.monotonic() - started > seconds:
                    reason = 'timeout'
                elif log.stat().st_size > 8 * 1024**2:
                    reason = 'log_guard'
                if reason:
                    stop_group(child)
                    break
                time.sleep(.2)
            try:
                status = child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stop_group(child, signal.SIGKILL)
                status = child.wait()
    except BaseException:
        reason = reason or 'supervisor_error'
        if child is not None:
            stop_group(child)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stop_group(child, signal.SIGKILL)
                child.wait()
        raise
    finally:
        if child is not None:
            stop_group(child, signal.SIGKILL)  # clean any surviving descendants
        for sig, handler in old.items():
            signal.signal(sig, handler)
        status = child.returncode if child is not None else None
        report = {'label': label, 'exit_code': status, 'stop_reason': reason,
                  'elapsed_seconds': time.monotonic() - started,
                  'sampled_group_rss_peak_kib': peak, 'rss_budget_mib': rss_mib,
                  'sampling_interval_seconds': .2,
                  'rss_scope': 'process_group_sum_shared_pages_may_be_counted_twice',
                  'minimum_observed_available_memory_kib': min_available,
                  'minimum_observed_free_bytes': free, 'wall_budget_seconds': seconds,
                  'ru_maxrss_kib_cumulative': resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
                  'cancelled_signal': cancelled[-1] if cancelled else None}
        (evidence / f'{label}.json').write_text(json.dumps(report, indent=2) + '\n')
        if log.exists() and log.stat().st_size > 2 * 1024**2:
            with log.open('rb') as source:
                source.seek(-2 * 1024**2, 2)
                tail = source.read()
            log.write_bytes(b'[truncated to final 2 MiB]\n' + tail)
    if cancelled:
        raise InterruptedError('diagnostic cancelled')
    return report


def checked(*args, **kwargs):
    report = run(*args, **kwargs)
    if report['exit_code'] != 0 or report['stop_reason'] is not None:
        raise RuntimeError(f"{report['label']} incomplete: {report['stop_reason'] or report['exit_code']}")
    return report
