# Workers candidate migration (not approved for production)

The production site remains GitHub Pages at `amx.j4ger.dev`. This branch neither
changes DNS nor adds a custom domain or Worker route. `blog` and its `j4ger.dev`
domain are outside this migration. Do not merge or switch traffic until the
remote acceptance below is complete and the user approves that separate step.

## Evidence and current boundary

On 2026-11-15, remote main was `f42c2cf33d923875041f23c86498a7b2f2a45ae3`;
it is already an ancestor of experiment `8e5426b4e4660bf87b1c6e543c7450f4d058da47`.
[Experiment CI run 43704875414](https://github.com/j4ger/animatix/actions/runs/43704875414)
succeeded. Cloudflare build `41f64d9b-d64b-41d5-a644-a382c3c0ce22` verified the
signature, built Astro using the signed SDK, emitted both PASS markers in 28s,
and intentionally exited 1. It did not deploy. Its schema-1 build-only permit
cannot pass the new schema-2 candidate policy, and neither permit authorizes
production activation.

The existing `animatix` trigger is still the experiment's failure-only probe.
Its default deploy command is `npx wrangler deploy`; its registered credential
is named `blog build token`. Do not reuse it or change its build command to a
successful candidate command. The registered tokens inspected were for `blog`
and `gemini-proxy`; no dedicated preview upload token was available.

## Candidate pipeline

1. `ci.yml` runs once on the dedicated `feat/cloudflare-candidate-preview` branch.
   The existing Nix Web Build produces one `web-runtime` SDK artifact. After
   `CI Required`, the optional `cloudflare_candidate` job signs the original ZIP's
   digest/size, artifact ID, repository ID, exact source SHA and run/attempt.
   Enable repository variable `ANIMATIX_CF_CANDIDATE=enabled` only when ready for
   this validation. No reusable full-CI wrapper or `workflow_run` ambiguity.
2. `cf-preview-<SHA>` is a non-latest prerelease. It contains the original ZIP,
   schema-2 permit and Sigstore bundle. Existing tags are rejected. Storage can
   change; SHA256 and the signed permit are the trust boundary, not the tag.
3. A **new** Workers Builds integration for `animatix-candidate-preview` checks
   out that exact SHA after the release exists. Build command:
   `bash scripts/ci.sh gate cloudflare-candidate-build`.
   Set `NODE_VERSION=24.14.0` and `SKIP_DEPENDENCY_INSTALL=1`.
   The consumer strips inherited credentials, pins gh and the TUF root snapshot,
   verifies signer/ref/source/run, rejects expired permits, checks SDK source
   fingerprint and archive bytes, and runs the existing Astro assembly checks.
4. Deploy **and non-production Preview** commands must both call
   `bash scripts/ci.sh gate cloudflare-candidate-upload` (or disable automatic
   preview builds). Never retain either platform default. The upload step
   re-verifies the signature and TTL, original ZIP and every assembled SDK byte,
   checks the fixed Worker/account/config, then runs pinned Wrangler
   `versions upload`. It does not call `deploy` or `versions deploy`.
5. Record the returned immutable version ID/Version URL together with
   `target/cloudflare-candidate/site.json` and signed permit. Do not use a mutable
   alias for acceptance. Check version metadata via the API before associating
   the receipt. Build/upload logs alone are not an activation receipt.

Use a separately named Worker with no bindings to production resources. Version
URLs use a version's existing resources; they are not resource isolation on their
own. Here isolation comes from the dedicated static-only Worker. Preview URLs
are public. No sensitive data belongs in the artifact.

This branch stages the code/configuration, not a provisioned remote integration.
A push build may happen before CI publishes the release and must fail closed;
after publication, explicitly trigger the exact SHA. Automatic dispatch is not
yet wired for the new trigger. Do not copy the experiment dispatcher: it mutates
and restores its shared probe command and expects an intentional failure.

The existing full CI still validates Astro locally. After remote acceptance is a
required, reliably reported candidate gate, split its fast policy/content checks
from Astro compilation to avoid that residual duplicated work. Do not remove
validation before its replacement has been exercised. The expensive Nix SDK is
already reused without a second build.

## Activation protocol and disorder

Candidate uploads never activate. Cloudflare's deployment POST does not expose
an expected-current-version CAS in the reviewed API. A fresh branch-head check,
GitHub concurrency, an expiring lock or an arbitrary run-number order cannot
fence a delayed older deployment POST.

`scripts/cloudflare-promotion.py` is an executable **journal prototype**, not a
live coordinator or deploy adapter. It demonstrates the required protocol:

- One durable, strongly consistent journal; explicit reviewed selection uses CAS
  on a monotonic epoch. Build completion cannot select itself. A future automatic
  selector must serialize authenticated desired-revision events independently
  of build completion, including force-push handling; never sort commit hashes.
- Persist pending intent before the external call. Exactly one ticket is issued.
  Keep pending across process death, HTTP timeout and unknown response. No later
  selection or activation may pass while pending, and never automatically retry
  an ambiguous POST. Seeing an old version on a GET is not proof the POST failed.
- Positive terminal reconciliation must bind deployment ID, version, candidate
  receipt and ticket. Confirming consumes the selection. An unresolved request
  stays blocked until its final outcome is conclusively established.
- Rollback selects an already accepted version/receipt with a **new** epoch;
  it cannot lower the fence. Retain that version's complete asset set.

The tests cover stale selection, old A finishing after B, competing connections,
crash-persistent uncertainty, wrong tickets, duplicate completion and rollback.
Production additionally requires an authenticated service around this journal,
verified receipt ingestion and version/asset binding, a sole credential holder,
remote reconciliation, backups and audited rollback authorization. Do not use a
fresh SQLite file in each Actions job. Do not treat this model as proof of a
production service. There is deliberately no production deploy endpoint here.

Workers Scripts upload permissions can also permit deployment; script convention
alone is not an IAM fence. Before production, isolate upload credentials from
production activation authority (for example an audited credential broker that
only permits validated version uploads) and exclude all alternate deploy writers.
A coordinator cannot fence dashboard or external writers that bypass it.

## Minimum missing access

Provision through Cloudflare's token UI/Builds integration; never paste keys into
chat. A dedicated user-owned build token for this preview Worker needs Workers
Scripts Edit (API: Workers Scripts Write, or supported Individual Workers Editor)
for version/asset upload and necessary reads. Scope to this Worker where supported;
otherwise the target account scope is broader and must be reviewed explicitly.
No DNS, Zone Workers Routes, KV, R2 or Pages write permission is needed for the
static-only `workers.dev` preview. Do not reuse the blog token.

Build trigger setup/dispatch needs account Workers Builds Configuration Edit
(API Workers CI Write). `CLOUDFLARE_EXPERIMENT_TOKEN` has only that grant and is
not an upload token. The Cloudflare connector's own API access is distinct from
a credential injected into a Workers Builds runner. No new token was created.

## Acceptance before merge

- Provision only the dedicated Worker/Builds trigger and record its identifier.
  Confirm no custom domains/routes or production resource bindings exist.
- Exercise real schema-2 positive Sigstore verification and expired/tampered
  rejection; upload two accepted versions from their exact CI artifacts.
- Run `python3 scripts/cloudflare-preview-check.py VERSION_URL site.json` for
  both: every HTML page/deep link, all runtime bytes/WASM partitions, MIME types,
  gzip format and the exact custom 404. Receipt must come from the trusted build,
  not be downloaded from the site being tested.
- Run the repository browser/WebGPU smoke and transactions tests against that
  origin, including capability loading, rendering, fonts and console errors.
  HTTP checks and local Astro tests do not establish browser/WebGPU correctness.
- Activate B then roll back to A only on the dedicated preview Worker; verify both
  receipts over HTTP and browser. Exercise A finishing late, concurrent selection,
  timeout after POST, restart and rollback fence with the real coordinator.
- Retain the Pages workflow, artifact and DNS snapshot as the production fallback.
  Production cutover is a separate user-approved operation after all evidence.

## Existing CNAME conflict: unresolved until isolated-host test

The [Custom Domains reference](https://developers.cloudflare.com/workers/configuration/routing/custom-domains/)
says a hostname with an existing CNAME cannot be attached. The
[Vercel migration guide](https://developers.cloudflare.com/workers/static-assets/migration-guides/vercel-to-workers/)
describes replacement of an existing provider DNS record. Those are not enough
to promise replacement of `amx.j4ger.dev` safely. Before any cutover, re-read both,
inspect the actual CNAME/certificate/settings, and perform an add/remove/restore
experiment on a separately authorized disposable hostname with a known CNAME.
Record actual API/UI behavior and DNS/TLS recovery. This branch does not create
that hostname or modify DNS. The old Pages guide describes a different product.

References: [Build configuration](https://developers.cloudflare.com/workers/ci-cd/builds/configuration/),
[Version URLs](https://developers.cloudflare.com/workers/versions-and-deployments/version-urls/),
[Rollback](https://developers.cloudflare.com/workers/versions-and-deployments/rollbacks/),
[Static Assets](https://developers.cloudflare.com/workers/static-assets/).

## Validation of this implementation

Local pinned Nix gates passed: `fmt`, `workflows`, `structure`, `doc-links`,
`site-build` (including existing permit/dispatch/transport regressions and Astro
check/tests/root and `/animatix/` builds). New tests exercise candidate-policy
separation and upload configuration rejection, and promotion journal disorder.

Pinned Wrangler 4.162.0 `deploy --dry-run` passed with 238 static files and no
bindings, using the previously assembled site as a **packaging fixture**, not a
new signed candidate. Local `wrangler dev --local` could not start because this
workspace returned `uv_interface_addresses: Unknown system error 1`; the attempted
HTTP smoke consequently had no server. No HTTP, browser, remote schema-2 signature,
remote upload, activation or rollback pass is claimed. No full GitHub CI rerun
was triggered merely to compensate for missing preview credentials.
