#!/usr/bin/env python3
"""Validate gate/job environment parity; called by the workflows gate."""
import sys

ALLOWED_SHELLS = {"native", "nix", "web", "web-ci", "dist-driver", "none"}
ALLOWED_STAGES = {"pr", "release", "site"}

def parse_rows(text, ncols, label):
    rows = []
    for lineno, line in enumerate(text.strip().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) != ncols:
            print(f"error: {label} row {lineno} has {len(parts)} columns, expected {ncols}: {line}",
                  file=sys.stderr)
            sys.exit(1)
        rows.append(parts)
    return rows

def setup_tokens(text):
    # `none` means "no setup", not a token the job must also list.
    tokens = set(text.split())
    tokens.discard("none")
    return tokens

gates = []
for name, shell, os_name, setup, stage in parse_rows(sys.argv[1], 5, "GATE_TABLE"):
    if shell not in ALLOWED_SHELLS:
        print(f"error: gate '{name}' has unknown shell '{shell}'", file=sys.stderr)
        sys.exit(1)
    if stage not in ALLOWED_STAGES:
        print(f"error: gate '{name}' has unknown stage '{stage}'", file=sys.stderr)
        sys.exit(1)
    if os_name == "ubuntu-latest" and (shell not in {"nix", "web", "web-ci", "dist-driver"} or setup != "nix"):
        sys.exit(f"error: Linux gate {name} must use a Nix environment")
    if (shell == "dist-driver") != (name == "dist-linux"):
        sys.exit("error: only dist-linux may use the isolated Docker driver")
    gates.append({
        "name": name, "shell": shell, "os": os_name, "stage": stage,
        "setup": setup_tokens(setup),
    })

jobs = []
for name, gate_list, shell, os_name, setup, stage in parse_rows(sys.argv[2], 6, "JOB_TABLE"):
    if shell not in ALLOWED_SHELLS:
        print(f"error: job '{name}' has unknown shell '{shell}'", file=sys.stderr)
        sys.exit(1)
    if stage not in {"pr", "release"}:
        print(f"error: job '{name}' has stage '{stage}'; JOB_TABLE jobs are pr/release only",
              file=sys.stderr)
        sys.exit(1)
    jobs.append({
        "name": name, "gates": gate_list.split(), "shell": shell, "os": os_name,
        "stage": stage, "setup": setup_tokens(setup),
    })

seen = set()
for gate in gates:
    key = (gate["name"], gate["shell"], gate["os"], gate["stage"], tuple(sorted(gate["setup"])))
    if key in seen:
        print(f"error: duplicate GATE_TABLE row for {gate['name']} on {gate['os']}", file=sys.stderr)
        sys.exit(1)
    seen.add(key)

def compatible(gate, job):
    return (gate["shell"] == job["shell"] and gate["os"] == job["os"]
            and gate["stage"] == job["stage"]
            and gate["setup"].issubset(job["setup"]))

bad = False
for job in jobs:
    for gate_name in job["gates"]:
        candidates = [g for g in gates if g["name"] == gate_name]
        if not candidates:
            print(f"error: job '{job['name']}' schedules unknown gate '{gate_name}'", file=sys.stderr)
            bad = True
            continue
        if not any(compatible(g, job) for g in candidates):
            print(f"error: job '{job['name']}' ({job['shell']}/{job['os']}/{job['stage']}, "
                  f"setup={sorted(job['setup'])}) is incompatible with gate '{gate_name}'", file=sys.stderr)
            for g in candidates:
                print(f"  gate row: {g['shell']}/{g['os']}/{g['stage']} setup={sorted(g['setup'])}",
                      file=sys.stderr)
            bad = True

for gate in gates:
    if gate["stage"] == "site":
        if any(gate["name"] in job["gates"] for job in jobs):
            print(f"error: site-stage composite gate '{gate['name']}' must not be a JOB_TABLE job",
                  file=sys.stderr)
            bad = True
        continue
    if not any(gate["name"] in job["gates"] and compatible(gate, job) for job in jobs):
        print(f"error: gate '{gate['name']}' ({gate['shell']}/{gate['os']}/{gate['stage']}) "
              f"has no compatible JOB_TABLE row", file=sys.stderr)
        bad = True

if bad:
    sys.exit(1)
site_gates = sorted({g["name"] for g in gates if g["stage"] == "site"})
print("GATE_TABLE and JOB_TABLE parity verified (shell/os/setup/stage)")
if site_gates:
    print("site-stage composite gates (run by name, not matrix jobs): " + ", ".join(site_gates))
