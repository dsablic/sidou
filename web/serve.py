# SPDX-FileCopyrightText: 2026 SIDOU contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Serve the demo (build/web) on localhost: http.server with the types the page needs and no
caching, so a rebuild shows on reload. fm1-emu's page needs no COOP/COEP headers: it uses no
SharedArrayBuffer (frames reach the page by postMessage).

    python3 web/serve.py build/web 8642    (port 0: any free port; the first line says which)
"""

import functools
import http.server
import sys
from typing import ClassVar


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map: ClassVar[dict[str, str]] = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
        ".json": "application/json",
        ".fwsc": "application/octet-stream",
    }

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


def main() -> None:
    directory, port = sys.argv[1], int(sys.argv[2])
    handler = functools.partial(Handler, directory=directory)
    with http.server.ThreadingHTTPServer(("127.0.0.1", port), handler) as server:
        print(f"http://127.0.0.1:{server.server_address[1]}/", flush=True)
        server.serve_forever()


if __name__ == "__main__":
    main()
