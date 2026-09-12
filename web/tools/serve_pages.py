#!/usr/bin/env python3
"""Serve a `@sveltejs/adapter-static` output the way GitHub Pages does.

Python's `http.server` does not resolve extensionless URLs, so `/docs/fxc` —
which adapter-static writes as `docs/fxc.html` — 404s, and `/docs` redirects to
a directory listing. Verifying the build against that would report breakage the
real host does not have, and worse, could hide breakage it does have.

GitHub Pages tries, in order: the path itself, `<path>.html`, then
`<path>/index.html` (redirecting to the directory form when the last one wins).
This does the same, so the local check exercises the routing the deploy will.

    python3 tools/serve_pages.py <build-dir> [port]
"""

import http.server
import os
import socketserver
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else '.'
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8798


class PagesHandler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)

    def translate_path(self, path):
        resolved = super().translate_path(path)
        if os.path.exists(resolved):
            if os.path.isdir(resolved):
                index = os.path.join(resolved, 'index.html')
                if os.path.exists(index):
                    return index
                # A directory with no index of its own: fall back to the sibling
                # `<name>.html` rather than listing the directory.
                sibling = resolved.rstrip('/') + '.html'
                if os.path.exists(sibling):
                    return sibling
            return resolved
        if os.path.exists(resolved + '.html'):
            return resolved + '.html'
        return resolved

    def log_message(self, *args):
        pass


socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(('127.0.0.1', PORT), PagesHandler) as httpd:
    print(f'serving {os.path.abspath(ROOT)} on http://127.0.0.1:{PORT}', flush=True)
    httpd.serve_forever()
