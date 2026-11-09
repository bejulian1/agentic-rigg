#!/usr/bin/python3 -I
"""Én forespørsel til den private tjenesten: rigg_klient.py OP [JSON-ARGS]."""
import json, os, socket, sys
sock = os.path.join(os.environ["XDG_RUNTIME_DIR"], "agentic-rigg", "riggd-dev.sock")
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw")
def req(i, op, args):
    f.write(json.dumps({"id": i, "op": op, "args": args}) + "\n"); f.flush()
    while True:
        m = json.loads(f.readline())
        if m.get("id") == i:
            return m
req(1, "hello", {"protocol": [1], "client": "test-3b/1"})
print(json.dumps(req(2, sys.argv[1], json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}), ensure_ascii=False))
