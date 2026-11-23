#!/usr/bin/env python3
"""Privat fixture: en GPU-hjelper fra et eldre bygg, bare for skjermbilder.

Svarer på `status` som en aktiv agentic-rigg-helper med oppgitt bygg (for
eksempel 31), så tjenesten melder «Oppdater hjelper». Ingen maskinvare, ingen
privilegier og ingen skriving utenfor roten. Roten må være en eksisterende,
egen 0700-mappe (den samme som RIGG_HJELPER_ROT for økta); vertens /run og
/etc brukes aldri.
"""
import argparse
import json
import os
from pathlib import Path
import selectors
import signal
import socket
import stat
import sys

MAX_LINE = 65_536


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--build', required=True, type=int)
    args = parser.parse_args()
    root = args.root.resolve()
    info = root.stat()
    if root == Path('/') or not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700:
        sys.exit('Fixturen krever en egen 0700-rot (aldri vertens).')
    run = root / 'run/agentic-rigg'
    etc = root / 'etc/agentic-rigg'
    run.mkdir(parents=True, exist_ok=True)
    etc.mkdir(parents=True, exist_ok=True)
    card = {'pci': '0000:1c:00.0', 'uuid': 'GPU-sim0000-0000'}
    (etc / 'helper.json').write_text(json.dumps({'versjon': 2, 'uid': os.getuid(), 'enabled': True, **card, 'bygg': args.build}))
    sock_path = run / 'helper.sock'
    sock_path.unlink(missing_ok=True)
    status = {
        'helper': {'name': 'agentic-rigg-helper', 'version': '1.5.6', 'build': args.build, 'protocol': 2,
                   'strictRequests': True, 'backend': 'nvml'},
        'identity': card,
        'authorized': True,
        'fixture': 'falsk-gammel-hjelper',
    }
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(str(sock_path))
    os.chmod(sock_path, 0o600)
    server.listen(4)
    server.setblocking(False)
    sel = selectors.DefaultSelector()
    sel.register(server, selectors.EVENT_READ)
    buffers = {}
    stop = []
    signal.signal(signal.SIGTERM, lambda *_: stop.append(1))
    while not stop:
        for key, _ in sel.select(timeout=0.5):
            if key.fileobj is server:
                conn, _ = server.accept()
                conn.setblocking(False)
                sel.register(conn, selectors.EVENT_READ)
                buffers[conn] = b''
                continue
            conn = key.fileobj
            try:
                data = conn.recv(8192)
            except OSError:
                data = b''
            if not data or len(buffers[conn]) > MAX_LINE:
                sel.unregister(conn)
                buffers.pop(conn, None)
                conn.close()
                continue
            buffers[conn] += data
            while b'\n' in buffers[conn]:
                line, buffers[conn] = buffers[conn].split(b'\n', 1)
                try:
                    request = json.loads(line)
                except ValueError:
                    continue
                result = status if request.get('op') == 'status' else {}
                reply = json.dumps({'id': request.get('id'), 'ok': True, 'result': result}) + '\n'
                try:
                    conn.sendall(reply.encode())
                except OSError:
                    break
    sock_path.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
