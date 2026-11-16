# Cloudflare Pages migration

The target is **Pages Git integration**, not Workers Builds. The Astro site is
static; Pages does not need Wrangler, a Worker upload token or Worker versions.
Rust/Wasm stays in the existing Nix CI. Pages only downloads the original signed
SDK, builds Astro and publishes `_site`.

## Current state

`amx.j4ger.dev` is served by the `animatix-pages` Cloudflare Pages project.
GitHub `j4ger/animatix` is connected with production branch `deploy/pages`.
Main CI signs the tested SDK; the sole publisher advances that branch and waits
for its Pages deployment. `deploy/pages-state` holds the durable release journal.
Both branches are infrastructure state, not disposable development branches.

Automatic production/preview builds in Pages stay disabled. The configured
build command is `bash scripts/ci.sh gate cloudflare-pages-build`, output `_site`,
with `NODE_VERSION=24.14.0` and `SKIP_DEPENDENCY_INSTALL=1`. The release job makes
one explicit deployment request. GitHub Pages is not the active host.

The normal path has passed real CI signing, Pages build, SDK verification and
custom-domain acceptance. That does not claim every failure-recovery or rollback
scenario has been exercised remotely. A Workers schema-2 permit is deliberately
not accepted by Pages.

## Signed build

1. Push `main` while repository variable `ANIMATIX_CF_PAGES` is
   `enabled`. Existing CI runs once and produces its original `web-runtime` ZIP.
2. Only after `CI Required` succeeds, `cloudflare_pages` signs a schema-3 permit
   for the exact source SHA, artifact hash/size, CI run/attempt and Pages project.
   It publishes `cf-pages-SHA-rRUN-aATTEMPT`, a non-latest prerelease. Storage is
   not assumed immutable; every consumer checks the signed digest.
3. The issuing branch and publishing branch differ intentionally: the former
   supplies the signed CI identity; `deploy/pages` selects an already-accepted
   commit without another full CI run.
4. Pages configuration: command `bash scripts/ci.sh gate cloudflare-pages-build`,
   output `_site`, repository root, `NODE_VERSION=24.14.0`,
   `SKIP_DEPENDENCY_INSTALL=1`. Keep preview branches disabled. There are no
   deployment credentials in the build environment.
5. The consumer rejects a wrong Pages branch or `CF_PAGES_COMMIT_SHA`, dirty
   source, expired permit, wrong signer or SDK bytes, and non-static output.
   It checks the actual publishing branch HEAD before and after building. Those
   checks are defense in depth, **not** an atomic deployment lock. A newly
   visible release with missing assets cannot hide an older still-valid issuance
   for the same commit: every fallback repeats signature, identity, TTL and SDK
   digest validation. Builds with no complete valid issuance fail closed.

## Release ordering and recovery

Do not treat the Pages deployment API's `commit_hash` field as checkout control:
its Git `branch` input builds that branch's HEAD. Never advance `deploy/pages`
while a previous deployment is queued or running. The journal-backed publisher
below owns ref advancement, one deployment request, and terminal reconciliation.
Do not run a second manual publisher beside it.

After a release, verify the CI source SHA, journal, `deploy/pages` and successful
canonical Pages deployment agree. Test the project/custom hostname and relevant
SDK or browser journeys; a build status alone is not browser acceptance.

## Domain and rollback

The custom domain is `amx.j4ger.dev`; the project hostname is
`animatix-pages.pages.dev`. Domain changes belong to Pages Custom Domains and DNS
administration, not the release job. Do not alter `j4ger.dev` or the `blog` Worker
as part of ordinary site publication.

Pages rollback selects a previously successful **production** deployment;
preview deployments cannot be promoted through rollback. Pause the publisher
before an operator rollback and reconcile its journal and canonical state before
reenabling it. Do not describe a rollback as verified until actually tested.
GitHub Pages is disabled and is not an immediately available rollback target.
Its repository workflow remains a separately configured manual fallback.

References: [Git integration](https://developers.cloudflare.com/pages/configuration/git-integration/),
[build configuration](https://developers.cloudflare.com/pages/configuration/build-configuration/),
[Pages deployments API](https://developers.cloudflare.com/api/resources/pages/subresources/projects/subresources/deployments/methods/create/),
[rollbacks](https://developers.cloudflare.com/pages/configuration/rollbacks/).

## Main publisher

The issuer and CI signing condition now agree on `refs/heads/main`. The original
feature-branch permits cannot authorize a fresh main build. Keep
`ANIMATIX_CF_PAGES=enabled` to sign successful main CI artifacts. The separate
`ANIMATIX_CF_PAGES_RELEASE=enabled` enables the release job only after signing.
GitHub Pages automatic publishing is removed; its manual fallback requires
`ANIMATIX_GH_PAGES_FALLBACK=enabled` and still runs its checks.

The release job needs a dedicated `CLOUDFLARE_PAGES_TOKEN` repository secret with
Pages Edit for the account. Do not reuse a Workers upload token or assume it has
Pages permission. No DNS editing is needed by this job. GitHub's job token uses
Contents Write to fast-forward `deploy/pages` and maintain the journal; Pages
Git integration must retain repository read access. Keep the Cloudflare project
production auto-build switch off and preview setting `none`: the job performs
one explicit deployment request after moving the branch.

For a new installation, before enabling release, dispatch **Initialize Pages release journal** on `main`
(`cloudflare-pages-initialize.yml`). It uses the repository secret inside Actions,
validates project access, disabled automatic builds, and an idle successful
canonical deployment, then creates `deploy/pages-state` and its journal. It never
requests a deployment or changes `deploy/pages`. A repeat is a read-only success
only when the existing journal exactly matches that completed deployment; drift,
unknown outcomes, and partial initialization fail closed without repair. Successful
initialization proves authenticated Pages reads and GitHub journal writes, not
Pages deployment-write permission; the first controlled release verifies that.
Keep both release variables off during initial provisioning until initialization
succeeds. Do not reinitialize the existing production journal for each release.

This branch is infrastructure state, not a
source branch. Protect it and `deploy/pages` from force pushes/deletion and all
other publishers, including manual Dashboard deployments and the old operator
procedure. Do not put credentials into the journal.

The Actions concurrency group serializes live job invocations only. Durability
comes from the journal, whose Contents API updates use the preceding file SHA:

- `preparing`: no deployment request was sent; safely recover a partial ref move.
- `intent`: persisted before POST; if its response is lost, freeze rather than
  resend or advance another commit. **There is no `--adopt` or reset command.**
  Real Pages Git API tests showed that a supplied `commit_message` ticket is
  replaced by the Git commit message. SHA, timestamps and a matching success
  therefore cannot prove which request a deployment belongs to. Even an
  operator-selected same-SHA ID cannot safely unlock this transaction. Resolve
  through provider evidence that all possible requests are settled, or retire
  this project/publisher through a separately reviewed recovery plan. An empty
  listing is not proof that POST did not execute. Do not erase the journal.
- `waiting`: record the ID and reconcile the same build after cancellation or
  timeout. No subsequent candidate advances until this build is terminal.
- `complete`: require the successful build to be canonical. Skip a candidate
  whose SHA is no longer main, and never rewind the release branch.
- `blocked`: a failed/cancelled deployment stops automatic progression. Inspect
  logs and all deployments before an operator uses `--recover-failed`. It
  verifies the recorded failure is terminal and no builds remain active. Do not delete
  or blindly reinitialize the journal to clear a failure.

This design does not prevent an account administrator or another credential from
bypassing the sole publisher. Keeping those external writers stopped is a
necessary operational condition, not an IAM guarantee made by this code. It also
fails closed rather than guaranteeing unattended recovery after ambiguous API
writes. Pages rollback is an explicit operator operation to an earlier successful
production deployment; afterward reconcile the journal/canonical state before
reenabling the publisher. Do not attempt preview promotion with rollback.

Local fault-injection tests cover duplicate/stale requests, POST response loss,
receipt-write loss, runner restart, failure, active foreign builds, partial ref
moves, SHA mismatch and canonical drift. These are not a substitute for remote
acceptance of API permissions and recovery. Normal publication has been accepted;
new installations must separately verify access before enabling release, and
unexercised failure paths must not be advertised as remotely validated.
