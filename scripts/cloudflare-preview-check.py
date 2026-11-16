#!/usr/bin/env python3
"""Read-only HTTP acceptance against a reviewed site receipt; never activates."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import urllib.error
import urllib.parse
import urllib.request


class SameOrigin(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if urllib.parse.urlsplit(req.full_url)[:2] != urllib.parse.urlsplit(newurl)[:2]:
            raise ValueError('preview redirected outside its origin')
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def verify(base, receipt):
    url = urllib.parse.urlsplit(base)
    if url.scheme != 'https' or not url.hostname or url.username or url.password or url.query or url.fragment or url.path not in ('', '/'):
        raise ValueError('expected an HTTPS preview origin')
    opener = urllib.request.build_opener(SameOrigin())
    files = receipt['files']
    required = {'index.html', '404.html', 'learn/compose-a-scene/index.html',
                'playground/index.html', 'runtime/runtime-manifest.json'}
    wasm = {f for f in files if f.endswith(('.wasm', '.wasm.gz'))}
    if not wasm or not required <= files.keys():
        raise ValueError('receipt lacks pages or WASM')
    def get(path):
        req = urllib.request.Request(base.rstrip('/') + '/' + path, headers={'Accept-Encoding': 'identity'})
        try:
            response = opener.open(req, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            body = response.read(25*1024**2+1)
            if len(body) > 25*1024**2:
                raise ValueError('response exceeds static asset limit')
            return response.status, response.headers, body
    # Check every recorded asset, including all capability WASM partitions and HTML routes.
    selected = sorted(files)
    for name in selected:
        path = name[:-10] if name.endswith('index.html') else name
        status, headers, body = get(urllib.parse.quote(path))
        if status != (404 if name == '404.html' else 200) and not (name == '404.html' and status == 200):
            raise ValueError('unexpected status for ' + name)
        if hashlib.sha256(body).hexdigest() != files[name]:
            raise ValueError('deployed bytes differ for ' + name)
        if name.endswith('.html') and 'text/html' not in headers.get('Content-Type', ''):
            raise ValueError('incorrect HTML MIME type')
        if name.endswith('.wasm') and 'application/wasm' not in headers.get('Content-Type', ''):
            raise ValueError('incorrect WASM MIME type')
        if name.endswith('.wasm.gz') and not gzip.decompress(body).startswith(b'\0asm'):
            raise ValueError('invalid compressed WASM')
    status, _, body = get('__animatix_missing_acceptance__/deep-link')
    if status != 404 or hashlib.sha256(body).hexdigest() != files['404.html']:
        raise ValueError('missing route does not serve exact 404 page')
    print(f'PREVIEW_HTTP_PASS pages/runtime={len(selected)} wasm={len(wasm)} source={receipt["source_sha"]}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('origin')
    parser.add_argument('receipt', type=Path)
    args = parser.parse_args()
    verify(args.origin, json.loads(args.receipt.read_text()))
