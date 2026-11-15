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
2. `cf-preview-<SHA>-r<RUN_ID>-a<ATTEMPT>` is a non-latest prerelease. It contains the original ZIP,
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

Automatic dispatch is implemented in `cloudflare-candidate-dispatch.py`, as a
separate job **after** signing/publication, never as a second full-CI workflow.
It is off unless `ANIMATIX_CF_PREVIEW_DISPATCH=enabled` and the reviewed target
file `.github/cloudflare/preview-target.json` is enabled with real identities.
The dispatcher verifies target tag, trigger, dedicated build-token ID, repository,
branch, commands and Node variables before one exact-SHA POST. It never mutates
trigger settings. It inspects the actual build metadata again when polling.
Wrong/default deploy commands fail closed. Existing experiment/blog/other-app
identifiers are explicitly rejected.

Set up the dedicated integration without an automatic pre-CI deployment path.
A push build that runs before its signed release exists must fail closed. Both
Builds command variants must use the verified uploader; do not retain a platform
default. `versions upload` never activates the Worker, even when builds finish
out of order. No mutable preview alias is requested.

The uploader records `_candidate.json` with the exact site inventory, then emits
one bounded `CANDIDATE_UPLOAD_RECEIPT` containing the version ID, immutable URL,
source SHA, permit/SDK/manifest digests and target Worker. The dispatcher retains
this alongside the build UUID in the Actions recovery artifact.

A dispatcher rerun cannot assume the previous POST failed. Later attempts read
complete authenticated per-attempt Actions job histories (`actions: read`),
including every page. A fresh POST is allowed only when every earlier attempt
proves the dispatch job or its exact trigger step was skipped, or the job was
absent from a complete nonempty inventory. Missing history, an executed step,
or an uncertain outcome blocks a new POST. Recover the build UUID from the
retained artifact and inspect it without posting another build:

```sh
python3 scripts/cloudflare-candidate-dispatch.py --resume-build BUILD_UUID
```

Run from the exact candidate checkout with the Builds-only token injected via
`CLOUDFLARE_EXPERIMENT_TOKEN`. If the initial POST result was lost before its UUID
was recorded, inspect Builds through the dashboard/API and establish the exact
build before resuming; there is no automatic blind retry. Release publication
is never overwritten. Rerun only the signing job to publish a new run/attempt
issuance using the same retained SDK artifact, without rerunning the SDK CI.
Discovery checks up to the newest eight issuances for the same exact SHA; each
release tag must match its verified permit identity. Build and upload still
enforce the current two-hour TTL. Late admission verifies the original signature
and TTL at the authenticated Builds completion time, and requires the exact
version creation time inside that authenticated build interval (at most 30
minutes). Missing provider timestamps fail closed. Renewal does not authorize
a blind retry of a potentially executed dispatch or any activation.

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

`scripts/cloudflare-release-coordinator.py` is now an operational local CLI with
a real API adapter. The earlier pure journal prototype has been removed. It is
**not deployed or enabled**. It is restricted in code to the separately named
preview Worker; there is no production target flag or automatic activation job.

- The private, owned `0700` state directory is the administration boundary. No
  HTTP management listener is exposed. A non-expiring OS file lock serializes
  processes on one host for the entire operation, including remote requests.
  SQLite FULL synchronization persists selection, pending intent, accepted
  candidate evidence and an append-only event log.
- `admit` authenticates the completed Builds metadata and single receipt, fetches
  the exact Worker version and rejects non-asset bindings. It verifies the
  Sigstore permit against pinned roots and CLI, compares the original archive
  digest/size and every SDK file to the deployed inventory, then checks every
  site file, HTML route, WASM and custom 404 over the exact Version URL. The
  manifest is trusted only after its digest matches authenticated build logs;
  it is not trusted merely because the site served it. Full signed bundle,
  permit and manifest are retained in the admitted journal record.
- Admission never changes the desired version. A reviewed `select` performs CAS
  on the expected monotonic epoch. No GitHub run-number or SHA sorting is used.
  Old A arriving after B can be registered but cannot select or activate itself.
- `activate` requires an admitted, selected receipt and the exact epoch. It
  checks for external deployment drift and re-reads the version's resource
  bindings, persists `prepared`, then persists `sent` **before** the sole POST.
  The request carries a unique ticket in `workers/message`. There are no POST
  retries and no expiration-based lock stealing.
- `reconcile` accepts only the exact ticket, version at 100%, strategy and
  deployment ID, confirmed by both detail and current-deployment reads. A lost
  success response can therefore recover without another POST. Confirming
  consumes the selection. A changed/wrong ticket or old deployment cannot unlock.
- A crash while durable state is only `prepared` can safely abandon that unsent
  intent under the exclusive lock. A crash after `sent`, even just before the
  POST, stays frozen unless positive terminal evidence arrives. All HTTP errors
  after send-intent conservatively remain uncertain. There is no unsafe reset or
  force-unlock command. Permanent rejection may therefore need provider/operator
  investigation; availability is intentionally sacrificed to prevent rollback
  by an old in-flight request.
- Rollback uses the same select/activate protocol with a **new** epoch and a
  previously admitted immutable version. Previously admitted permits need not
  be reissued merely because the short build/upload TTL expired. The platform
  must still retain that version/assets; missing versions fail the preflight.

The pinned Wrangler 4.162.0 source uses the first deployment returned by the
list endpoint as current (`fetchLatestDeployment`) and the first eight version
ID characters for Version URLs. The adapter follows those same contracts and
fails closed if required response fields are absent. Real API acceptance is
still required; mocked response contracts are not a remote integration result.

### Local coordinator runbook (preview only)

First provision and independently review a new static-only Worker with a known
baseline deployment (or explicitly empty deployment inventory), no domain/routes/production bindings, and its dedicated
Builds trigger. Fill and review `preview-target.json`; it currently contains no
live target IDs and `enabled` is false. Install the pinned gh using the existing
attestation-tools helper. Inject the coordinator credential through the process
environment as `CLOUDFLARE_PREVIEW_COORDINATOR_TOKEN`, never CLI arguments.

Use an existing absolute `0700` directory on durable local storage. Do not use
an ephemeral CI checkout, a copied database on a second host or NFS locking.
`init` is only for a genuinely new single-writer installation, never recovery
from lost state or a possibly active old coordinator:

```sh
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview init --new-single-writer-installation
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview admit --build BUILD_UUID --gh /opt/animatix-tools/gh_2.104.0_linux_amd64/bin/gh
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview status
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview select --expected-epoch 0 --candidate RECEIPT_DIGEST --reviewed-browser-and-receipt
```

After reviewing the receipt and browser/WebGPU results, explicitly enable
`ANIMATIX_CF_PREVIEW_ACTIVATION=enabled` only for the isolated preview acceptance:

```sh
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview activate --epoch 1 --candidate RECEIPT_DIGEST
python3 scripts/cloudflare-release-coordinator.py --state-directory /srv/animatix-preview reconcile
```

`reconcile` is for a pending request, not an extra required step after a confirmed
activation. Status reports the current epoch, desired/active candidate and any
pending ticket. To roll back, select the old admitted digest using the current
epoch, then activate the returned new epoch. Never recreate/delete the journal
to clear a timeout. Back up the journal/evidence under the same exclusive lock;
restoring an older snapshot is not safe while any old writer/request may exist.

### Remaining safety boundary

The CLI is usable with provisioned access, but it is not yet a proven production
service. The target identity, remote API response shape, real signed schema-2
admission, HTTP/browser acceptance, activation and rollback remain untested on
Cloudflare. Existing custom-domain/zone-route absence must be independently
verified before enabling this preview writer; the coordinator does not enumerate
all zones. Network redirects and unknown responses never widen the target.

The sole-writer requirement must be enforced operationally and with credentials:
there must be one durable host/state directory and no alternate deployment writer.
A local file lock cannot fence a second machine or a dashboard/API token holder.
A stale snapshot restore loses pending intent. Until these conditions and the
real fault/recovery drills are verified, activation remains disabled by default.
Automating **selection** as well as upload is intentionally withheld: it needs an
authenticated desired-release sequence independent of build completion. Explicit
reviewed selection is available now. Production activation remains out of scope.

Workers Scripts upload permissions can also permit deployment; script convention
alone is not an IAM fence. Before production, isolate upload credentials from
production activation authority (for example an audited credential broker that
only permits validated version uploads) and exclude all alternate deploy writers.
A coordinator cannot fence dashboard or external writers that bypass it.

## Minimum missing access

Provision through Cloudflare's token UI/Builds integration; never paste keys into
chat. Permission names alone do not establish a narrowly scoped usable credential:

- The [Workers authorization reference](https://developers.cloudflare.com/workers/authorization/workers/)
  says an **existing** Worker needs Editor for version upload/deploy/rollback;
  creating a new Worker needs product-level Admin. Legacy Workers Scripts Edit
  corresponds to product-level Editor across all Workers, not single-Worker scope.
- Individual Worker Editor and fine-grained API-token scope are documented, but
  the fine-grained Wrangler token guidance and Builds token types must be tested
  together: the Builds configuration page currently says **user tokens only**.
  Do not promise that a single-Worker/account-owned token can be selected in
  Builds or can upload successfully. Determine the actual minimum only through
  the UI and a real isolated upload. A broader account token requires explicit
  review, particularly while `blog` exists in that account.
- Builds setup/dispatch needs the Builds Configuration Edit/API Workers CI Write
  capability for the target account; the existing experiment secret supplies
  that known grant, not demonstrated version-upload authority. The coordinator
  needs Builds read access for authenticated admission plus Worker version and
  deployment access for the dedicated target. Keep its credential separate from
  the GitHub dispatcher. Do not reuse blog/gemini-proxy credentials.
- Static-only `workers.dev` version uploads do not require DNS/Zone Routes, KV,
  R2 or Pages writes. New Worker bootstrap permissions are a separate operation.
  The connector's permissions do not automatically become a Builds-runner token.

No token, Worker, trigger, domain or route was created by this implementation.

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
separation and upload configuration rejection, and the actual control path under the operational fault-injection tests. Those tests mock
Cloudflare and positive cryptographic verification; existing negative signature
tests remain, but no new real schema-2 signature pass is claimed.

Pinned Wrangler 4.162.0 `deploy --dry-run` passed with 238 static files and no
bindings, using the previously assembled site as a **packaging fixture**, not a
new signed candidate. Local `wrangler dev --local` could not start because this
workspace returned `uv_interface_addresses: Unknown system error 1`; the attempted
HTTP smoke consequently had no server. No HTTP, browser, remote schema-2 signature,
remote upload, activation or rollback pass is claimed. No full GitHub CI rerun
was triggered merely to compensate for missing preview credentials.


### Review fixes and first Worker initialization

Wrangler 4.162.0 `versions upload` requires an existing Worker. Provisioning
`animatix-candidate-preview` is a separate explicit operation, using a reviewed
static forwarding module and no production domain/routes. Record its Worker tag
and any initial deployment as the coordinator baseline before enabling Builds.
Do not interpret a not-found response as an empty deployment inventory; do not
silently substitute `wrangler deploy` in the candidate uploader. Creation needs
product-level authority; an existing-Worker editor role and Builds user-token
compatibility must be verified in the UI and by an actual isolated upload.

The candidate config now uses an explicit, unbundled ASSETS forwarding module.
Admission obtains deployed module bytes through the authenticated Workers API
with `include=modules`, compares their independently pinned SHA256, and requires
exactly the ASSETS binding and reviewed asset routing. Zero bindings or a fetch
handler declaration alone is not evidence of static behavior. Unsupported API
shape, missing content or timestamps blocks admission; the live endpoint and
upload remain acceptance blockers until tested with the isolated Worker.

The local lock serializes only processes using the same durable coordinator
state directory. It cannot fence another deployment token, Dashboard operation,
or a restored/cloned state directory. Exclusive writer ownership is an external
installation prerequisite. Drift detection is diagnostic, not a Cloudflare-side
CAS. Activation remains disabled if that ownership cannot be established; no
production-safe global ordering claim is made.


Review validation: `fmt`, `workflows`, `structure`, `doc-links`, and `site-build`
passed after these fixes (8 review regressions and 17 coordinator/transport tests,
plus the existing policy, packaging and Astro checks). Wrangler 4.162.0 local
`deploy --dry-run` packaged the 238-file site fixture with only ASSETS. Its emitted
`static-worker.js` exactly matched the reviewed source SHA256
`2e35546260a53329466e6640dad5bc694c94391a7ea2e165f175c5b85a788517`.
This is local packaging evidence, not a signed candidate upload or HTTP/browser
acceptance. No remote deploy, DNS change, main merge or full CI rerun occurred.
