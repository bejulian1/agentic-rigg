#!/usr/bin/python3 -I
"""Et vindu uten GTK: abonnerer på prøvene (1 s), sier at Disker vises, og
spør get_disks hvert 15. s, som Disker-siden. klient.py SOCKET SEKUNDER"""
import json, socket, sys, time
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
f = s.makefile("rw")
n = 0
def send(op, args):
    global n
    n += 1
    f.write(json.dumps({"id": n, "op": op, "args": args}) + "\n")
    f.flush()
send("hello", {"protocol": [1], "client": "maal-6a/1"})
send("subscribe", {"topics": ["telemetry", "stats"], "intervalMs": 1000})
send("set_focus", {"pages": ["disker"]})
end = time.time() + float(sys.argv[2])
last = 0.0
s.settimeout(0.5)
while time.time() < end:
    if time.time() - last >= 15:
        send("get_disks", {})
        last = time.time()
    try:
        f.readline()
    except (socket.timeout, OSError):
        pass
