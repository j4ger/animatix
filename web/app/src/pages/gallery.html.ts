import {url} from '../lib/content';

// Preserve the exact former filename on static hosts, without server redirects.
export const prerender = true;
export function GET() {
  return new Response(`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>The collection has moved · Animatix</title><meta name="description" content="Find current Animatix work and earlier studies."><style>html{color-scheme:dark;background:#101315;color:#eeeae3;font:18px/1.6 system-ui,sans-serif}body{max-width:700px;margin:12vh auto;padding:24px}h1{font-size:clamp(36px,6vw,64px);line-height:1.1;letter-spacing:-.04em}p{color:#a4aaa9}a{color:#d2f5a1;text-underline-offset:5px}nav{display:flex;flex-wrap:wrap;gap:24px}a:focus-visible{outline:2px solid #d2f5a1;outline-offset:5px}</style></head><body><a href="${url('')}">animatix.</a><main><h1>A collection, not a catalog.</h1><p>The old gallery has been replaced by a smaller collection of complete stories. Earlier studies remain documented in the archive.</p><nav aria-label="Choose a destination"><a href="${url('showcase/')}">Watch current work →</a><a href="${url('learn/')}">Learn by doing →</a><a href="${url('archive/')}">Find an earlier study →</a></nav></main></body></html>`, {headers: {'Content-Type': 'text/html; charset=utf-8'}});
}
