#!/usr/bin/env python3
"""Collect canonical scene diagnostics against an explicit zero-debt budget.

Used by the canonical-audit CI gate and local triage. Never updates a checked-in budget.
Exit 0 means all audits passed; 1 means findings/failures; 2 means setup failure.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

from audit_corpus import ROOT, inventory


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def checkpoint(output, records, expected, binary_hash, complete=False, binary_unchanged=None,
               ci_summary=None):
    # Raw diagnostics remain local. Only these typed counters/booleans and the
    # executable digest may be uploaded: no text, paths, commands or environment.
    state = dict(schema_version=1, complete=complete, expected=expected,
                 completed=len(records), passed=sum(r['passed'] for r in records),
                 binary_sha256=binary_hash, binary_unchanged=binary_unchanged)
    (output / 'progress.json').write_text(json.dumps(state, indent=2) + '\n')
    if ci_summary is not None:
        ci_summary.parent.mkdir(parents=True, exist_ok=True)
        temporary = ci_summary.with_suffix('.tmp')
        temporary.write_text(json.dumps(state, indent=2) + '\n')
        temporary.replace(ci_summary)


def collect(binary, output, ci_summary=None):
    entries = inventory()
    # Require a fresh directory: old reports must never masquerade as this run.
    output.mkdir(parents=True, exist_ok=False)
    budget = output / 'zero-budget.json'
    budget.write_text(json.dumps(dict(schema_version=3, policy='default', render='never', files={})))
    binary_before = sha256(binary)
    records = []
    checkpoint(output, records, len(entries), binary_before, ci_summary=ci_summary)
    for index, entry in enumerate(entries):
        source = ROOT / entry['path']
        command = [str(binary), 'audit', entry['path'], '--render', 'never',
                   '--policy', 'default', '--format', 'json', '--budget', str(budget)]
        record = dict(entry, source_sha256=sha256(source), command=command)
        prefix = output / f'{index:02d}-{source.stem}'
        try:
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=180)
            prefix.with_suffix('.json').write_text(result.stdout)
            prefix.with_suffix('.stderr.txt').write_text(result.stderr)
            report = json.loads(result.stdout)
            summary = report.get('summary') if isinstance(report, dict) else None
            budget_report = report.get('budget') if isinstance(report, dict) else None
            valid = (isinstance(report, dict) and report.get('schema_version') == 4
                     and report.get('kind') == 'audit' and isinstance(summary, dict)
                     and type(summary.get('errors')) is int and summary['errors'] == 0
                     and type(summary.get('warnings')) is int and summary['warnings'] == 0
                     and isinstance(budget_report, dict)
                     and budget_report.get('exceeded') == []
                     and budget_report.get('updated') is False)
            record.update(returncode=result.returncode, summary=summary, budget=budget_report,
                          passed=valid and result.returncode == 0)
        except subprocess.TimeoutExpired as exc:
            for suffix, value in (('.json', exc.stdout), ('.stderr.txt', exc.stderr)):
                prefix.with_suffix(suffix).write_bytes(value.encode() if isinstance(value, str) else value or b'')
            record.update(passed=False, diagnostic=f'timeout after {exc.timeout}s; output may be partial')
        except (OSError, ValueError, AttributeError) as exc:
            record.update(passed=False, diagnostic=str(exc))
        try:
            record['source_unchanged'] = sha256(source) == record['source_sha256']
        except OSError:
            record['source_unchanged'] = False
        record['passed'] = record['passed'] and record['source_unchanged']
        records.append(record)
        checkpoint(output, records, len(entries), binary_before, ci_summary=ci_summary)
        print(('PASS' if record['passed'] else 'REVIEW') + ': ' + entry['path'])
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    try:
        binary_after = sha256(binary)
    except OSError:
        binary_after = None
    binary_unchanged = binary_before == binary_after
    manifest = dict(source_commit=commit, binary=str(binary), binary_sha256=binary_before,
                    binary_sha256_after=binary_after, binary_unchanged=binary_unchanged,
                    provenance='Hash identifies the executable bytes, not its build source.',
                    budget='explicit zero debt; no updates', render='never', records=records)
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    checkpoint(output, records, len(entries), binary_before, complete=True,
               binary_unchanged=binary_unchanged, ci_summary=ci_summary)
    return 0 if binary_unchanged and all(item['passed'] for item in records) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin', type=Path, default=ROOT / 'target/debug/animatix')
    parser.add_argument('--ci-summary', type=Path, help='write upload-safe counters only')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not args.bin.is_file():
        parser.error('build animatix CLI first or specify --bin')
    try:
        return collect(args.bin.resolve(), args.output.resolve(),
                       args.ci_summary.resolve() if args.ci_summary else None)
    except (OSError, ValueError) as exc:
        print(f'error: {exc}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
