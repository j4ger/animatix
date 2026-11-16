# Retired Worker preview experiment

The Worker candidate upload/activation experiment has been retired. The site now
uses [Cloudflare Pages](cloudflare-pages.md), with signed SDK verification and a
serialized Pages release journal. No Wrangler Worker configuration is required.

The retired configuration and implementation remain available in
[the pre-retirement source](https://github.com/j4ger/animatix/tree/530b5f8843d31a77b19540ff8f42daf662fddfff).
Do not re-enable the old candidate CI variables or reuse its upload commands.

This cleanup removes repository entrypoints only; it does not delete remote
Workers, tokens, GitHub secrets, releases or deployment history.
