#!/usr/bin/env python3
"""Prøv GTK mot et utsatt hello-svar. Bruk: python3 test-oppgaver-hello.py STI_TIL_AGENTIC_RIGG."""

import ctypes
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time


def x11():
    """Gi GTK EWMH-tilstand fra en vindusbehandler på den private X-serveren."""
    lib = ctypes.CDLL("libX11.so.6")
    ptr, xid, integer = ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int
    for name, args, result in (
        ("XOpenDisplay", [ctypes.c_char_p], ptr),
        ("XDefaultRootWindow", [ptr], xid),
        ("XCreateSimpleWindow", [ptr, xid, integer, integer, integer, integer, integer, xid, xid], xid),
        ("XInternAtom", [ptr, ctypes.c_char_p, integer], xid),
        ("XChangeProperty", [ptr, xid, xid, xid, integer, integer, ptr, integer], integer),
        ("XSync", [ptr, integer], integer),
    ):
        fn = getattr(lib, name)
        fn.argtypes, fn.restype = args, result
    display = lib.XOpenDisplay(None)
    assert display, "Fant ikke den private X-serveren"
    root = lib.XDefaultRootWindow(display)
    manager = lib.XCreateSimpleWindow(display, root, -100, -100, 1, 1, 0, 0, 0)

    def atom(name):
        return lib.XInternAtom(display, name.encode(), False)

    def prop(window, name, kind, *values):
        data = (xid * len(values))(*values)
        lib.XChangeProperty(display, window, atom(name), atom(kind), 32, 0, data, len(data))
        lib.XSync(display, False)

    prop(root, "_NET_SUPPORTING_WM_CHECK", "WINDOW", manager)
    prop(manager, "_NET_SUPPORTING_WM_CHECK", "WINDOW", manager)
    prop(root, "_NET_SUPPORTED", "ATOM", atom("_NET_WM_STATE"), atom("_NET_WM_STATE_HIDDEN"))

    def minimize(window, hidden):
        states = [atom("_NET_WM_STATE_HIDDEN")] if hidden else []
        prop(window, "_NET_WM_STATE", "ATOM", *states)

    return minimize


def check(conn, minimize):
    conn.settimeout(0.1)
    messages = []
    data = b""

    def reply(request, result):
        response = {"id": request["id"], "ok": True, "result": result}
        conn.sendall(json.dumps(response).encode() + b"\n")

    def read_for(seconds, until=lambda: False):
        nonlocal data
        end = time.monotonic() + seconds
        while time.monotonic() < end and not until():
            try:
                chunk = conn.recv(65536)
            except TimeoutError:
                continue
            assert chunk, "GTK lukket tilkoblingen"
            data += chunk
            while b"\n" in data:
                line, data = data.split(b"\n", 1)
                request = json.loads(line)
                messages.append(request)
                if request["op"] != "hello":
                    reply(request, {"gpus": [], "loading": True, "rows": []})

    read_for(3, lambda: any(m["op"] == "hello" for m in messages))
    hello = next(m for m in messages if m["op"] == "hello")
    tree = subprocess.check_output(["xwininfo", "-root", "-tree"], text=True)
    match = re.search(r'(0x[0-9a-f]+) "Oppgaver', tree)
    assert match, tree
    window = int(match[1], 16)
    minimize(window, True)
    read_for(0.5)
    messages.clear()
    reply(hello, {"daemon": {"backend": "ingen"}})
    read_for(2.5)
    focus = [m["args"]["pages"] for m in messages if m["op"] == "set_focus"]
    assert focus and all(pages == [] for pages in focus), messages
    assert not any(m["op"] == "get_tasks" for m in messages), messages

    messages.clear()
    minimize(window, False)
    read_for(2, lambda: {m["op"] for m in messages} >= {"get_tasks", "set_focus"})
    assert any(m["op"] == "get_tasks" for m in messages), messages
    assert any(m["op"] == "set_focus" and m["args"]["pages"] == ["oppgaver"] for m in messages), messages


def run(folder):
    minimize = x11()
    socket_path = str(folder / "socket")
    env = os.environ | {
        "GDK_BACKEND": "x11",
        "GTK_A11Y": "none",
        "XDG_RUNTIME_DIR": str(folder / "run"),
        "RIGG_PROFIL": "dev",
        "RIGG_SOCKET": socket_path,
        "RIGG_CONFIG_DIR": str(folder / "config"),
        "RIGG_STATE_DIR": str(folder / "state"),
        "RIGG_HJELPER_ROT": str(folder / "hjelper"),
    }
    with socket.socket(socket.AF_UNIX) as listener:
        listener.bind(socket_path)
        listener.listen()
        listener.settimeout(10)
        with subprocess.Popen([folder / "agentic-rigg", "--side", "oppgaver"], env=env) as app:
            try:
                with listener.accept()[0] as conn:
                    check(conn, minimize)
            finally:
                app.terminate()


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--privat":
        run(Path(sys.argv[2]))
    else:
        if len(sys.argv) != 2:
            sys.exit(__doc__)
        with tempfile.TemporaryDirectory(prefix="rigg-hello-") as tmp:
            folder = Path(tmp)
            for name in ["run", "config", "state", "hjelper"]:
                (folder / name).mkdir(mode=0o700)
            # Uten daemonbinær ved siden av appen kan den ikke starte reservetjenesten.
            shutil.copy2(sys.argv[1], folder / "agentic-rigg")
            bus = folder / "bus.conf"
            bus.write_text("""<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>
<policy context="default"><allow send_destination="*"/><allow receive_sender="*"/>
<allow own="*"/></policy></busconfig>""")
            subprocess.run(
                ["xvfb-run", "-a", "dbus-run-session", f"--config-file={bus}", "--",
                 sys.executable, __file__, "--privat", tmp],
                check=True, timeout=20,
            )
        print("OK: Ingen polling etter hello mens vinduet var minimert. Polling startet ved gjenvisning.")
