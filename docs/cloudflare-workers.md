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


### Isolated account preparation observed on 2026-11-15

The dedicated empty Worker `animatix-candidate-preview` now exists in the Workers
API with ID `2084ce245de94d90aa46f7e33f8303c4`. Its `deployed_on` is null,
workers.dev and preview URLs are disabled, and its domain/resource references
are empty. The account workers.dev subdomain is `j4ger`. The legacy Scripts list
does not yet contain this empty Worker. Consequently `worker_tag` remains unset
until its Builds/legacy identity is verified; existence alone is not proof that
Wrangler versions upload can initialize it. No other Worker's token is reusable.

Before token configuration, the dedicated source branch, local tests, immutable
module packaging and exact build-command template can be prepared. Creating a
Builds trigger requires a dedicated `build_token_uuid` in the API request; it
cannot be completed with only the existing Builds Configuration Edit token.
After the user configures the dedicated token through Cloudflare, verify its
upload permission and token type compatibility without exposing its value, then
record the returned UUID, Worker tag and trigger UUID. Only then enable the
reviewed target and the two separate GitHub candidate/signing dispatch switches.
Keep activation disabled throughout upload and immutable-URL acceptance.


### Skipped-ancestor signing correction

For source `c3b4d23c0ea12de1487000a9c0a5c9995982dcc3`, CI run `43723595056`
passed and retained SDK artifact `14165977734` (archive SHA256
`439b504f402b5d7e34aaa920a6314f0a6198902348827ea5b054ad969ae1d0e7`).
The initial signer skipped. Re-running only CI Required with the signing switch
enabled produced a successful aggregate in attempt 2, but signer job
`132857274003` still skipped. Its log endpoint returned BlobNotFound, so the
exact expanded condition was unavailable; this is not evidence of a token error
or of an SDK failure.

There is a concrete condition defect: optional native-CLI/shared-render ancestors
normally skip, CI Required uses `always()`, but the downstream candidate jobs
had no status function. GitHub's implicit `success()` propagates skipped ancestors
through the chain even after a successful direct aggregate. Both candidate jobs
now use `!cancelled()` and retain explicit direct-dependency success and opt-in
checks. See [GitHub job dependency semantics](https://docs.github.com/actions/using-jobs/using-jobs-in-a-workflow).
This is a source-level fix; actual hosted signing still needs acceptance.

Repeating the old run cannot pick up changed workflow YAML. Do not repeat the
failed signing-only recovery suggestion for this old revision or rerun its full
matrix: both retain the broken condition. The safe continuation under the current
same-commit policy is one new branch CI run at the repaired commit, with only
`ANIMATIX_CF_CANDIDATE=enabled` set before pushing and preview dispatch disabled.
This runs the existing baseline matrix once, including one Nix Web SDK build,
then signs its original artifact. It does not run a second SDK build on
Cloudflare. After this corrected signer has actually run, signing-only renewal
can reuse that run's SDK; the aggregate remains a required success prerequisite.

The old SDK bytes cannot simply be relabeled for the repaired SHA: the current
policy binds site, SDK source, producer, checkout and attestation signer digest
to one SHA. Reuse across different source commits would require a separately
reviewed schema for SDK build-input identity and separate signer provenance.
That trust-model expansion is intentionally not smuggled into this recovery.
The previous SDK remains retained for comparison; no old signature is forged,
no TTL is removed, and no arbitrary source is accepted.

The dedicated trigger is now verified as
`afb53392-b40b-4a76-b9f9-71fd2bb3ffc1`, with Worker tag
`2084ce245de94d90aa46f7e33f8303c4` and registered `animatix-deploy` token UUID
`c863eb06-126a-42e1-8649-88d7b85dd7ea`. These identifiers are recorded with the
local target still disabled. Read-only diagnostic build
`58158f06-8e83-4350-a6e5-7df8c9f5ac2e` confirmed HTTP 200 on the isolated
Worker's legacy versions endpoint, then intentionally failed at `exit 1`.
All trigger settings were restored. Read access does not establish upload rights.


### First real signed candidate upload (2026-11-15)

Source `b35670b2b81e67eee72207e1bb1c940e6ccf09b0` passed CI run `43729866272`.
Original SDK artifact `14167674797` has SHA256
`0466f21ebd2e09c3289e8436206b9a53e3d81c770dcbd6faa67195efe83100d8`.
Cloudflare build `caba0c68-b907-4bd2-b24a-36e98bc89889` passed signed permit
verification, built Astro using that SDK, and uploaded version
`090415ea-b4ee-4650-877f-827683ed576c`. The dedicated user token can therefore
perform this upload into the pre-created empty Worker. This does not prove
creation rights or that a narrower token would work.

Independent local verification with the pinned gh/root snapshot authenticated
the permit at the provider's build completion time and verified both the permit
digest `e763955ae65e3acbe18d5ea5c6059427dbe227d1caa23087aed8826906f9891f`
and original SDK ZIP digest. Build/upload stages enforced current TTL themselves.
No activation occurred: the deployments list remained empty and deployed_on null.

The uploaded version is NOT admitted. Two live findings prevented acceptance:

- `no_bundle` implicitly discovers additional modules unless explicitly disabled.
  With npm installed under web/deploy, Wrangler attached ten tool dependency
  WASM/text files (about 9 MB) in addition to the reviewed forwarding module.
  The earlier dry-run borrowed Wrangler from outside this directory, so it did
  not exercise the real dependency layout. Exact module-count validation rejects
  this version. The fix sets `find_additional_modules=false`; a new local dry-run
  with dependencies installed in the actual directory emits only the identical
  172-byte static-worker.js. Strict admission was not loosened.
- Preview URL access was disabled on the Worker. `versions upload` did not enable
  it despite preview_urls=true in config; the receipt's preview_url was null.
  Upload now checks workers.dev=false and previews_enabled=true before writing,
  and rejects a missing returned URL after upload without retrying it. Enabling
  preview URLs requires explicit authorization because it makes versions
  externally accessible. Automatic approval review rejected the temporary
  enable operation; it was not executed at that stage. The subsequent authorized
  diagnostic is recorded below. No HTTP/WASM/deep-link/browser or
  rollback acceptance pass is claimed.

After testing, the trigger's original experiment branch, build/deploy `exit 1`,
and all-path exclusion were restored. Temporary NODE_VERSION was deleted;
SKIP_DEPENDENCY_INSTALL remains. workers.dev and previews remain disabled,
there are no domain bindings and no deployments. Keep the uploaded version as
non-admitted diagnostic evidence; do not promote it or silently repair its
receipt. These code fixes require a new authenticated source/build before remote
acceptance. No user-computer execution or new CI run was started for the fixes.


### Authorized public preview diagnostic (2026-11-15)

The user subsequently explicitly authorized temporarily public version URLs on
this isolated Worker. Enabling previews with workers.dev still disabled returned
an immutable URL for version `090415ea-b4ee-4650-877f-827683ed576c`.
The cloud browser showed Cloudflare's “There is nothing here yet” page, including
on reload. This is an observed availability failure, not evidence that activating
a deployment is required. The version remains rejected for its extra modules.

A distinct diagnostic enabled previews before uploading the same signed source,
without rebuilding the GitHub SDK. Build
`01557297-acb9-4ab3-96d1-d10f14e5078d` stopped at
`SIGNED_CONSUMER_BLOCKED: HTTP Error 403: rate limit exceeded`, before version
upload. No new version or activation resulted from that attempt. No blind retry
or weakened signature verification was used. Browser, WASM, deep-link and rollback
acceptance remain incomplete.

After both diagnostics, provider readback verified build/deploy commands `exit 1`,
original experiment branch and all-path exclusion, only the original
SKIP_DEPENDENCY_INSTALL environment variable, workers.dev=false,
previews_enabled=false, no deployments and deployed_on=null. Production domains
and routing were never changed.

The packaging/preflight repair is local commit
`3b195a21866b47927189d20f52b5b55a6e207a29`. A cloud GitHub create-tree request
containing only its four ordinary files returned HTTP 403 “Resource not accessible
by integration”; no remote tree, commit or ref was created. The remote branch
therefore remains at `b35670b2b81e67eee72207e1bb1c940e6ccf09b0`. The user computer
must not be used. A cloud connection authorized to write this repository is
needed to publish the fix, followed by a newly signed source build under the
existing exact-SHA policy. GitHub release download rate limits and the unverified
first-version URL behavior are separate remaining acceptance blockers.
