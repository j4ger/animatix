#!/usr/bin/env python3
"""Issue/verify a short-lived BUILD-ONLY experiment permit.

`emit` accepts metadata only from the trusted publisher after needs.ci succeeds;
its output is NOT trusted until separately attested. `verify` always invokes the
caller-provisioned, pinned gh binary with a local bundle and trusted roots. It
never downloads tools/roots, reads credentials, calls an API or deploys anything.
The trusted-root file must be independently pinned, not trusted merely because
it accompanied an artifact. See GitHub's offline attestation verification guide.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

REPOSITORY = "j4ger/animatix"
REPOSITORY_ID = 1185977116
REF = "refs/heads/experiment/cloudflare-native-build"
WORKFLOW = ".github/workflows/cloudflare-sdk-experiment.yml"
SIGNER = f"https://github.com/{REPOSITORY}/{WORKFLOW}@{REF}"
ACCOUNT = "65bc5958d8a01d7a1d9b7c326b244406"
TRIGGER = "59dc8a02-0b69-42e9-9c04-f76ac35f8b82"
GH_VERSION = "2.104.0"
TTL = 2 * 60 * 60
MAX_ARCHIVE = 64 * 1024**2
PREDICATE = "https://slsa.dev/provenance/v1"


class PermitError(ValueError):
    """The candidate cannot authorize the requested isolated build."""


def require(condition, message):
    if not condition:
        raise PermitError(message)


def exact_keys(value, keys, label):
    require(isinstance(value, dict) and set(value) == set(keys), f"invalid {label} fields")


def integer(value, lower, upper, label):
    require(type(value) is int and lower <= value <= upper, f"invalid {label}")


def sha(value, length=40):
    require(isinstance(value, str) and re.fullmatch(rf"[0-9a-f]{{{length}}}", value), "invalid digest")


def reject_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON field")
        result[key] = value
    return result


def decode(data):
    try:
        return json.loads(data, object_pairs_hook=reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PermitError("invalid JSON") from error


def read_limited(path, limit):
    with Path(path).open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "input file exceeds limit")
    return data


def accept_result(expected_latest_sha, candidate_sha):
    """Reject stale results; NOT an atomic production deployment/CAS guarantee.

    The caller must supply a fresh authoritative expected SHA. A later state
    change can race this pure comparison, so deployment requires a separate
    serialized/fenced promotion transaction. No GitHub run-number ordering is
    inferred, and no production promotion is implemented here.
    """
    sha(expected_latest_sha)
    sha(candidate_sha)
    require(expected_latest_sha == candidate_sha, "stale build result")
    return True


def emit(metadata, now=None):
    """Construct unsigned permit from trusted CI wrapper metadata, not user JSON."""
    fields = {"repository_id", "repository", "ref", "workflow", "site_sha", "sdk_source_sha",
              "producer_sha", "run_id", "run_attempt", "needs_ci_result", "artifact_id",
              "archive_sha256", "archive_size_bytes"}
    exact_keys(metadata, fields, "publisher metadata")
    require(metadata["repository_id"] == REPOSITORY_ID and type(metadata["repository_id"]) is int,
            "wrong publisher repository ID")
    require(metadata["repository"] == REPOSITORY and metadata["ref"] == REF
            and metadata["workflow"] == WORKFLOW, "wrong publisher identity")
    now = int(time.time()) if now is None else now
    permit = {
        "schema_version": 1,
        "mode": "build-only",
        "repository": {"id": REPOSITORY_ID, "name": REPOSITORY},
        "ref": REF,
        "site_sha": metadata["site_sha"],
        "sdk_source_sha": metadata["sdk_source_sha"],
        "producer_sha": metadata["producer_sha"],
        "ci": {"workflow": WORKFLOW, "run_id": metadata["run_id"],
               "run_attempt": metadata["run_attempt"], "needs_ci_result": metadata["needs_ci_result"]},
        "sdk": {"artifact_id": metadata["artifact_id"], "archive_sha256": metadata["archive_sha256"],
                "archive_size_bytes": metadata["archive_size_bytes"]},
        "cloudflare": {"account_id": ACCOUNT, "trigger_uuid": TRIGGER},
        "issued_at": now,
        "expires_at": now + TTL,
    }
    validate_permit(permit, metadata["site_sha"], now=now)
    return permit


def validate_permit(permit, checkout_sha, now=None):
    """Validate claims after signature verification (also used before issuance)."""
    exact_keys(permit, {"schema_version", "mode", "repository", "ref", "site_sha", "sdk_source_sha",
                        "producer_sha", "ci", "sdk", "cloudflare", "issued_at", "expires_at"}, "permit")
    require(type(permit["schema_version"]) is int and permit["schema_version"] == 1, "unsupported schema")
    require(permit["mode"] == "build-only", "permit cannot authorize deployment")
    exact_keys(permit["repository"], {"id", "name"}, "repository")
    require(type(permit["repository"]["id"]) is int and permit["repository"]["id"] == REPOSITORY_ID
            and permit["repository"]["name"] == REPOSITORY, "wrong repository")
    require(permit["ref"] == REF, "wrong experiment ref")
    sha(checkout_sha)
    for key in ("site_sha", "sdk_source_sha", "producer_sha"):
        sha(permit[key])
        require(permit[key] == checkout_sha, "site/SDK/producer/checkout SHA mismatch")
    exact_keys(permit["ci"], {"workflow", "run_id", "run_attempt", "needs_ci_result"}, "CI")
    ci = permit["ci"]
    require(ci["workflow"] == WORKFLOW and ci["needs_ci_result"] == "success", "CI prerequisite not successful")
    integer(ci["run_id"], 1, 2**63 - 1, "run ID")
    integer(ci["run_attempt"], 1, 1000000, "run attempt")
    exact_keys(permit["sdk"], {"artifact_id", "archive_sha256", "archive_size_bytes"}, "SDK")
    integer(permit["sdk"]["artifact_id"], 1, 2**63 - 1, "artifact ID")
    sha(permit["sdk"]["archive_sha256"], 64)
    integer(permit["sdk"]["archive_size_bytes"], 1, MAX_ARCHIVE, "archive size")
    exact_keys(permit["cloudflare"], {"account_id", "trigger_uuid"}, "Cloudflare")
    require(permit["cloudflare"] == {"account_id": ACCOUNT, "trigger_uuid": TRIGGER}, "wrong experiment destination")
    now = int(time.time()) if now is None else now
    integer(now, 0, 2**53 - 1, "current time")
    integer(permit["issued_at"], 0, 2**53 - 1, "issued time")
    integer(permit["expires_at"], 0, 2**53 - 1, "expiry")
    require(permit["expires_at"] - permit["issued_at"] == TTL, "permit lifetime must be two hours")
    require(permit["issued_at"] <= now + 60 and now < permit["expires_at"], "permit expired or issued in future")
    return permit


def validate_verified_output(results, permit, permit_bytes):
    """Policy over SUCCESSFUL gh JSON output, never over an unverified bundle.

    gh emits [{attestation: ..., verificationResult: {signature: {certificate:
    {...}}, statement: {...}, verifiedTimestamps: [...]}}]. Certificate fields
    follow sigstore-go JSON (not a nested `extensions` object). Source:
    https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/verify-attestations-offline
    https://github.com/sigstore/sigstore-go/blob/main/docs/verification.md
    """
    require(isinstance(results, list) and 1 <= len(results) <= 8, "missing verified attestation")
    expected_sha = permit["producer_sha"]
    expected_invocation = (f"https://github.com/{REPOSITORY}/actions/runs/"
                           f"{permit['ci']['run_id']}/attempts/{permit['ci']['run_attempt']}")
    digest = hashlib.sha256(permit_bytes).hexdigest()
    for result in results:
        require(isinstance(result, dict), "invalid verification result")
        verified = result.get("verificationResult", {})
        require(isinstance(verified, dict), "invalid verified result")
        signature = verified.get("signature", {})
        require(isinstance(signature, dict), "missing verified signature")
        cert = signature.get("certificate", {})
        require(isinstance(cert, dict), "missing verified certificate")
        expected = {"sourceRepositoryIdentifier": str(REPOSITORY_ID),
                    "sourceRepositoryURI": f"https://github.com/{REPOSITORY}",
                    "sourceRepositoryDigest": expected_sha, "sourceRepositoryRef": REF,
                    "issuer": "https://token.actions.githubusercontent.com",
                    "buildSignerURI": SIGNER, "buildSignerDigest": expected_sha,
                    "runnerEnvironment": "github-hosted", "runInvocationURI": expected_invocation}
        require(all(cert.get(key) == value for key, value in expected.items()), "verified signer identity does not match permit")
        require(isinstance(verified.get("verifiedTimestamps"), list) and verified["verifiedTimestamps"],
                "missing verified signing timestamp")
        statement = verified.get("statement", {})
        require(isinstance(statement, dict) and statement.get("predicateType") == PREDICATE, "unexpected signed predicate")
        subjects = statement.get("subject")
        require(isinstance(subjects, list) and len(subjects) == 1 and isinstance(subjects[0], dict)
                and subjects[0].get("digest") == {"sha256": digest}, "signed permit subject digest mismatch")
    return True


def verify_command(gh, permit, bundle, trusted_root, checkout_sha):
    sha(checkout_sha)
    return [str(gh), "attestation", "verify", str(permit), "--repo", REPOSITORY,
            "--bundle", str(bundle), "--custom-trusted-root", str(trusted_root),
            "--signer-workflow", f"{REPOSITORY}/{WORKFLOW}", "--signer-digest", checkout_sha,
            "--source-ref", REF, "--source-digest", checkout_sha, "--deny-self-hosted-runners",
            "--predicate-type", PREDICATE, "--format", "json"]


def verify(permit_path, bundle_path, trusted_root_path, gh_path, checkout_sha, latest_sha=None):
    """Offline cryptographic verification with pre-provisioned trusted tooling."""
    gh_path = Path(gh_path)
    require(gh_path.is_absolute() and gh_path.is_file() and os.access(gh_path, os.X_OK), "gh must be an absolute executable path")
    permit_bytes = read_limited(permit_path, 1024**2)
    read_limited(bundle_path, 16*1024**2)
    read_limited(trusted_root_path, 4*1024**2)
    # No secret/config inheritance, no GH_TOKEN, no default gh credentials.
    with tempfile.TemporaryDirectory(prefix="animatix-permit-verify-") as home:
        env = {"HOME": home, "GH_CONFIG_DIR": home, "PATH": os.defpath, "LANG": "C.UTF-8",
               "GH_PROMPT_DISABLED": "1", "GH_NO_UPDATE_NOTIFIER": "1"}
        version = subprocess.run([str(gh_path), "--version"], check=True, capture_output=True,
                                 text=True, timeout=15, env=env).stdout
        require(version.splitlines() and version.splitlines()[0].startswith(f"gh version {GH_VERSION} "),
                "unexpected gh version; provision the reviewed pin")
        result = subprocess.run(verify_command(gh_path, permit_path, bundle_path, trusted_root_path, checkout_sha),
                                check=True, capture_output=True, timeout=60, env=env)
    require(len(result.stdout) <= 32*1024**2, "verification output exceeds limit")
    # Parse claims only AFTER gh authenticated the exact permit bytes.
    permit = validate_permit(decode(permit_bytes), checkout_sha)
    validate_verified_output(decode(result.stdout), permit, permit_bytes)
    if latest_sha is not None:
        accept_result(latest_sha, permit["site_sha"])
    return permit


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    issuer = commands.add_parser("emit")
    issuer.add_argument("--metadata", required=True)
    issuer.add_argument("--output", required=True)
    verifier = commands.add_parser("verify")
    for name in ("permit", "bundle", "trusted-root", "gh", "checkout-sha"):
        verifier.add_argument("--" + name, required=True)
    verifier.add_argument("--latest-sha")
    args = parser.parse_args()
    try:
        if args.command == "emit":
            permit = emit(decode(read_limited(args.metadata, 1024**2)))
            # Refuse accidental overwrite; attestation must cover these bytes.
            with Path(args.output).open("x", encoding="utf-8") as out:
                out.write(json.dumps(permit, sort_keys=True, indent=2) + "\n")
        else:
            permit = verify(args.permit, args.bundle, args.trusted_root, args.gh,
                            args.checkout_sha, args.latest_sha)
            print(json.dumps(permit, sort_keys=True))
    except (OSError, PermitError, subprocess.SubprocessError) as error:
        # Do not echo gh stderr, raw metadata, credentials or a failed bundle.
        print(f"permit rejected: {type(error).__name__}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
