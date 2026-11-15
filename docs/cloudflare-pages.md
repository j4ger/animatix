# Cloudflare Pages deployment

Cloudflare is an additional deployment target, not a new build system. GitHub
Pages remains available during migration. No workflow changes DNS.

## Build and trust boundary

`CI` must succeed on `main`. The Cloudflare workflow checks out that exact SHA,
restores and verifies its `web-runtime` artifact, and assembles `_site` inside the
locked Nix environment. Only an absent/expired artifact permits the existing
controlled SDK rebuild. A corrupt or wrong-commit artifact fails closed.

The root-path static output is uploaded with Wrangler 4.162.0, locked separately
in `web/deploy/package-lock.json`. It does not bundle the SDK or replace Astro's
static output with a Worker. Deliberately update this pin with the lockfile and
rerun deployment checks; do not substitute `npx wrangler@latest` in CI.

The uploader is installed before the token-bearing step. The workflow only has
repository read permissions, and deployment refuses a dirty checkout, different
SDK/source SHA or a commit that is no longer remote `main`. A stale CI completion
must not roll production back. GitHub's existing workflow is unchanged.

## One-time setup (owner)

1. Create a Cloudflare **Pages / Direct Upload** project with production branch
   `main`. Direct Upload cannot later be converted to Git integration in place.
2. Set repository Actions variables `CLOUDFLARE_PAGES_PROJECT` and
   `CLOUDFLARE_ACCOUNT_ID`. Until the project variable exists, this workflow skips.
3. Create a token scoped to the intended account with **Cloudflare Pages: Edit**.
   Store it directly in GitHub Actions secret `CLOUDFLARE_API_TOKEN`. Do not paste
   it into a chat, issue, command history or tracked file. Deployment does not
   need DNS Edit, GitHub write access, or an account-wide global API key.
4. Run `Deploy site to Cloudflare Pages` on `main`, or let the next successful CI
   trigger it. Inspect deployment receipt and test the assigned `pages.dev` host.

No paid services, Functions, Workers script or external asset store is required
for the present output. Recheck provider quotas before substantially enlarging
media or Wasm. Preflight rejects any file above 25 MiB, over 20,000 files,
symlinks, missing 404/manifest, non-root base or accidental server-side code.

## Local verification

```sh
nix develop --command scripts/ci.sh gate fmt workflows site-build
# Requires an SDK verified against the current checkout; see web/README.md.
SITE_BASE=/ nix develop --command scripts/ci.sh gate site-artifact cloudflare-prepare
```

`cloudflare-prepare` is a no-secret check/install. `cloudflare-deploy` is an
intentional production mutation and is not part of ordinary PR checks.

The static 404 page is required: without a top-level `404.html`, Pages assumes
SPA routing and can return the home page for missing resources. This is a
multi-page static site. Test a missing page and a missing Wasm URL return 404.

## Domain cutover and rollback

First validate `pages.dev`: homepage, deep routes, old-route explanations, AMX
imports, fonts, Wasm, live WebGPU playback on capable hardware, videos and Range
requests. Native renders or recorded video do not prove live WebGPU works.

Then add `amx.j4ger.dev` in the Pages project's **Custom domains** UI before
changing its CNAME. Cloudflare-managed DNS can create the record through this
flow. Record the old DNS target/proxy settings, confirm the exact change, and
wait for the custom domain certificate to become active. Do not manually point
an unassociated hostname at `pages.dev` (it can return 522).

Keep the GitHub deployment and its domain binding during the verification
window. If cutover fails, restore the recorded DNS settings. Once Cloudflare
is confirmed healthy, remove the obsolete GitHub binding/deployment separately.
Pages can also roll production back to an earlier successful production deploy;
preview deployments are not rollback targets.

Cloudflare zone cache/WAF/redirect rules can still affect Pages. Avoid a broad
cache-everything rule for HTML or the runtime manifest. Fingerprinted assets can
be cached longer. Do not blindly set `Content-Encoding: gzip` on stored `.gz`
files: the SDK has its own Wasm/font decoding paths. Test response bytes and
integrity rather than assuming CDN compression is transparent to every loader.

## Official references

- [Direct upload from CI](https://developers.cloudflare.com/pages/how-to/use-direct-upload-with-continuous-integration/)
- [Direct Upload tradeoffs](https://developers.cloudflare.com/pages/get-started/direct-upload/)
- [Custom domains](https://developers.cloudflare.com/pages/configuration/custom-domains/)
- [Limits](https://developers.cloudflare.com/pages/platform/limits/)
- [Route and 404 behavior](https://developers.cloudflare.com/pages/configuration/serving-pages/)
- [Production rollbacks](https://developers.cloudflare.com/pages/configuration/rollbacks/)
