#!/usr/bin/env bash
# Serves web/ over HTTP with Cross-Origin-Opener-Policy/Cross-Origin-Embedder-Policy headers
# set, which is required for SharedArrayBuffer to be available at all (the mechanism the live
# input harness uses -- see platform::wasm's "Live input" section in rust/src/platform/wasm.rs
# and the plan doc's Phase B SharedArrayBuffer note). Plain `python3 -m http.server` can't set
# custom headers, hence this wrapper.
set -euo pipefail
cd "$(dirname "$0")/../web"
PORT="${1:-8642}"

python3 -c "
import http.server

class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

http.server.test(HandlerClass=Handler, port=$PORT)
"
