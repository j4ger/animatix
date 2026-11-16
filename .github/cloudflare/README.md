# Experimental attestation trust

`trusted-root.jsonl` was obtained on 2026-11-15 using the upstream GitHub CLI
2.104.0 `gh attestation trusted-root` command (TUF verification enabled).
SHA256: `65ca537f6ed8a47fd0e560c421baa1f6c1efb8b25fc200d8c5c02c0e92eb2b9c`.
The consumer pins this independently of the untrusted Release payload. Review
and update it intentionally when roots rotate; an offline snapshot cannot detect
later revocation. Do not replace it with a root downloaded beside a candidate.

The CLI Linux amd64 archive is pinned to
`98cb101ad87dbbd3f6b2f2bfcbd43197ccd07643140121d8a9cfaa91eadbc24e`, checked against
both the upstream Release API digest and its checksums file. This does not claim
the SDK Release itself is immutable. Permit policy is experiment-only and must
never be reused as a production deployment authorization.
