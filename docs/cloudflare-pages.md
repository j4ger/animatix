# Cloudflare Pages migration

The target is **Pages Git integration**, not Workers Builds. The Astro site is
static; Pages does not need Wrangler, a Worker upload token or Worker versions.
Rust/Wasm stays in the existing Nix CI. Pages only downloads the original signed
SDK, builds Astro and publishes `_site`.

## Current state

`animatix-pages` was created on 2026-11-15 with GitHub `j4ger/animatix`, production
branch `deploy/pages`, all automatic production/preview deployments disabled and
build command `exit 1`. No custom domain or DNS changed. The project has no
accepted deployment yet. Its default hostname is `animatix-pages.pages.dev`.

The code's initial issuer is `feat/cloudflare-pages`. After acceptance, changing
the issuer to `main` requires updating the policy and CI condition together and
running fresh main CI. A Workers schema-2 permit is deliberately not accepted.

## Signed build

1. Push `feat/cloudflare-pages` while repository variable `ANIMATIX_CF_PAGES` is
   `enabled`. Existing CI runs once and produces its original `web-runtime` ZIP.
2. Only after `CI Required` succeeds, `cloudflare_pages` signs a schema-3 permit
   for the exact source SHA, artifact hash/size, CI run/attempt and Pages project.
   It publishes `cf-pages-SHA-rRUN-aATTEMPT`, a non-latest prerelease. Restore the
   temporary variable after the job ends. Storage is not assumed immutable.
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
   checks are defense in depth, **not** an atomic deployment lock.

## First deployment and serial release procedure

Do not treat the Pages deployment API's `commit_hash` field as checkout control:
its Git `branch` input builds that branch's HEAD. Never advance `deploy/pages`
while a previous deployment is queued or running.

For the first deployment, a single operator owns the complete transaction:

1. Verify the signed release and exact CI SHA, record current release-branch SHA
   (or absence), current Pages canonical deployment ID, and all active builds.
2. Require no pending/running Pages build. Keep automatic production disabled.
3. Create/fast-forward `deploy/pages` to the accepted SHA using a conditional Git
   ref update. Reject non-ancestor changes. No force rewrites, mutable SDK aliases
   or cross-commit SDK relabeling.
4. Submit one Pages Git deployment for `deploy/pages`, record its returned ID,
   and wait for a terminal result. If the request outcome is unknown, inspect
   deployment history for that SHA before any retry; do not advance the branch.
5. Verify the returned build's commit SHA, production environment, successful
   stages and canonical deployment ID. Test its immutable deployment URL, then
   the project hostname: homepage, showcase, deep links, unknown path 404, SDK
   bytes/MIME, gzip fetch behavior, fonts and a browser animation/WebGPU check.
6. Only then may the next commit advance. Failure or uncertain state keeps the
   release sequence stopped. Retrying uses the same SHA and a current signed
   permit, not an older candidate.

This is currently an **operator-serialized** first-release procedure. There is
no claim that multiple independent automation writers are coordinated. Before
unattended release, implement and test a durable sole publisher that owns both
ref advancement and the entire Pages terminal wait/reconciliation; restrict
other writers. GitHub concurrency alone is not a commit-ordering or durable
lease mechanism. Automatic deployment remains disabled until that exists.

## Domain and rollback

After project-hostname acceptance, add `amx.j4ger.dev` through Pages Custom
Domains, inspect the existing DNS record first, and verify the resulting CNAME
and certificate. Do not alter `j4ger.dev` or the `blog` Worker. Record the old
GitHub Pages binding/DNS so rollback is concrete. Keep GitHub Pages available
until the new domain works, then disable its redundant automatic publishing.

Pages rollback selects a previously successful **production** deployment;
preview deployments are not promoted through rollback. On the first release,
there is no previous Pages production deployment, so the rollback is the old
GitHub Pages route. Test later Pages rollback before calling it verified.

## Acceptance still required

The new policy and gates need real CI signing and a Pages-hosted build. The
Workers experiment proves SDK portability, not Pages checkout/deployment order,
response behavior or domain readiness. Existing Worker experiments can remain
stopped until the Pages migration is accepted; their tokens are not Pages tokens.

References: [Git integration](https://developers.cloudflare.com/pages/configuration/git-integration/),
[build configuration](https://developers.cloudflare.com/pages/configuration/build-configuration/),
[Pages deployments API](https://developers.cloudflare.com/api/resources/pages/subresources/projects/subresources/deployments/methods/create/),
[rollbacks](https://developers.cloudflare.com/pages/configuration/rollbacks/).
