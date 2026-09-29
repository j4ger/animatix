#!/usr/bin/env python3
"""Local static server for the Animatix web demo with production-like serving:

- correct `application/wasm` MIME type (enables instantiateStreaming)
- transparent brotli negotiation: serves the precompressed `.br` twin when
  the client advertises `Accept-Encoding: br` (a CDN does the same)
- modest cache headers

Usage: python3 scripts/serve-web.py [port]   (default 8124, serves web/)
"""
import http.server
import os
import sys

WEB_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web")

MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".mjs": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json",
    ".wasm": "application/wasm",
    ".png": "image/png",
    ".svg": "image/svg+xml",
    ".amx": "text/plain; charset=utf-8",
    ".ttf": "font/ttf",
    ".otf": "font/otf",
}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=WEB_DIR, **kwargs)

    def guess_type(self, path):
        ext = os.path.splitext(path)[1].lower()
        return MIME.get(ext, "application/octet-stream")

    def send_head(self):
        # brotli negotiation: prefer the precompressed twin when present
        path = self.translate_path(self.path)
        accepts_br = "br" in self.headers.get("Accept-Encoding", "")
        if accepts_br and os.path.isfile(path) and not path.endswith(".br"):
            br_path = path + ".br"
            if os.path.isfile(br_path):
                # serve file.br with the original's type + Content-Encoding
                f = open(br_path, "rb")
                try:
                    self.send_response(200)
                    self.send_header("Content-Type", self.guess_type(path))
                    self.send_header("Content-Encoding", "br")
                    self.send_header("Content-Length", str(os.fstat(f.fileno()).st_size))
                    self.send_header("Cache-Control", "public, max-age=300")
                    self.send_header("Vary", "Accept-Encoding")
                    self.end_headers()
                    return f
                except Exception:
                    f.close()
                    raise
        return super().send_head()

    def end_headers(self):
        self.send_header("Cache-Control", "public, max-age=300")
        super().end_headers()

    def log_message(self, fmt, *args):
        sys.stderr.write("[serve-web] %s\n" % (fmt % args))


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8124
    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"serving web/ at http://127.0.0.1:{port}/")
    server.serve_forever()
