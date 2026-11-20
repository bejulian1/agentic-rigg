#!/usr/bin/env python3
"""Privat RGB-fixture for GTK-prøver. Ingen maskinvare eller privilegier.

Krever en eksisterende, kanonisk 0700-mappe under /tmp og en vanlig 0600-fil
rgb-flow.fake med den eksakte markøren nedenfor. Socket og sporfil opprettes
bare under denne roten. Dette skriptet er ikke en produksjonsbackend.
"""

import argparse
import copy
import colorsys
import json
import math
import os
from pathlib import Path
import selectors
import signal
import socket
import stat
import struct
import sys
import time
import uuid


MARKER = "agentic-rigg-rgb-fake-v1"
MAX_LINE = 65_536
MAX_OUTPUT = 262_144
MAX_CLIENTS = 4
MAX_TRACE = 16 * 1024 * 1024
LEASE_MS = 15_000
BASELINE = {"mode": "static", "color": [20, 40, 60], "brightness": 80}
SOFTWARE_EFFECTS = ("breathing", "rainbow", "cycle")
BINDING = {
    "pci": "0000:1c:00.0",
    "uuid": "GPU-sim0000-0000",
    "pciIds": {
        "vendor": 0x10DE,
        "device": 0x1E87,
        "subsystemVendor": 0x10DE,
        "subsystemDevice": 0x1E87,
    },
    "adapter": {
        "port": 1,
        "name": "NVIDIA i2c adapter 1 at 1c:00.0",
        "parentPci": "0000:1c:00.0",
        "driver": "nvidia",
    },
    "address": 73,
    "signature": "palit_150",
}


def now_ms():
    """Samme klokke som hjelperen, inkludert suspend; ingen veggklokkefallback."""
    return time.clock_gettime_ns(time.CLOCK_BOOTTIME) // 1_000_000


class RpcError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def fields(value, required, optional=()):
    if type(value) is not dict or not set(required) <= value.keys():
        raise RpcError("invalid_argument", "Forespørselen mangler påkrevde felt.")
    if value.keys() - set(required) - set(optional):
        raise RpcError("invalid_argument", "Forespørselen inneholder ukjente felt.")


def color_request(value):
    if type(value) is not dict:
        raise RpcError("invalid_argument", "Lysvalget må være et objekt.")
    if value.get("mode") == "off":
        fields(value, ("mode",))
    elif value.get("mode") == "static":
        fields(value, ("mode", "color", "brightness"))
        color = value["color"]
        if (type(color) is not list or len(color) != 3
                or not all(integer(v, 0, 255) for v in color)
                or not integer(value["brightness"], 0, 100)):
            raise RpcError("invalid_argument", "Ugyldig farge eller lysstyrke.")
    else:
        raise RpcError("invalid_argument", "Bare fast farge og av støttes i fixturen.")
    return copy.deepcopy(value)


def observed(request):
    return {
        "color": [0, 0, 0] if request["mode"] == "off" else request["color"][:],
        "brightnessRaw": 100 if request["mode"] == "off" else request["brightness"],
        "mode": None,
    }


def effect_frame(effect, seed, elapsed):
    """Visningsfixture: én statisk farge per bilde, aldri maskinvaretilgang."""
    target = copy.deepcopy(seed)
    if effect == "breathing":
        level = 0.1 + 0.9 * (1 + math.cos(elapsed * math.tau / 4000)) / 2
        target["color"] = [int(channel * level + 0.5) for channel in seed["color"]]
    else:
        hue, _sat, _value = colorsys.rgb_to_hsv(*(v / 255 for v in seed["color"]))
        offset = elapsed / 12_000 if effect == "rainbow" else (elapsed // 2000) / 6
        target["color"] = [int(v * 255 + 0.5) for v in colorsys.hsv_to_rgb((hue + offset) % 1, 1, 1)]
    return target


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplisert JSON-felt.")
        result[key] = value
    return result


def parse_json(line):
    def reject_constant(_):
        raise ValueError("JSON-tallet er ikke endelig.")
    value = json.loads(line, object_pairs_hook=unique_object, parse_constant=reject_constant)
    pending, count = [(value, 0)], 0
    while pending:
        node, depth = pending.pop()
        count += 1
        if depth > 32 or count > 8192:
            raise ValueError("JSON-strukturen er for stor eller for dyp.")
        if type(node) is dict:
            pending.extend((child, depth + 1) for child in node.values())
        elif type(node) is list:
            pending.extend((child, depth + 1) for child in node)
    return value


class FakeState:
    def __init__(self, build):
        self.build = build
        self.confirmed = copy.deepcopy(BASELINE)
        self.observed = observed(BASELINE)
        self.pending = None
        self.owner = None
        self.lease = None
        self.dirty = False
        self.may_have_written = False
        self.recovery = None
        self.confirmation_source = None
        self.error = None
        self.observation_source = "stored_baseline"
        self.last_outcome = None
        self.sequence = 1
        self.instance_id = uuid.uuid4().hex
        self.recovery_attempt = 0
        self.effect = None
        self.effect_seed = None
        self.effect_start = None
        self.effect_frame = None

    def recovery_id(self):
        if self.recovery == "readable_fields_match" and self.confirmation_source is None:
            return f"{self.instance_id}-{self.recovery_attempt}"
        return None

    def status(self):
        return {
            "testMarker": MARKER,
            "helper": {
                "name": "agentic-rigg-rgb-helper", "domain": "palit_rgb",
                "protocol": 2, "build": self.build, "version": "testfixture",
            },
            "identity": copy.deepcopy(BINDING),
            "authorized": True,
            "recoveryId": self.recovery_id(),
            "softwareEffects": list(SOFTWARE_EFFECTS),
            "effect": self.effect,
            "holding": None if self.owner is None else {
                "owner": self.owner, "deadlineMs": self.lease, "leaseMs": LEASE_MS,
            },
            "state": {
                "baselineSource": "user_confirmed_static",
                "confirmed": copy.deepcopy(self.confirmed),
                "observed": copy.deepcopy(self.observed),
                "pending": copy.deepcopy(self.pending), "owner": self.owner,
                "dirty": self.dirty, "mayHaveWritten": self.may_have_written,
                "recovery": self.recovery,
                "recoveryNeedsConfirmation": self.recovery == "readable_fields_match"
                    and self.confirmation_source is None,
                "recoveryConfirmationSource": self.confirmation_source,
                "error": self.error,
            },
            "observationSource": self.observation_source,
            "lastOutcome": copy.deepcopy(self.last_outcome),
            "runtimeError": None,
        }

    def outcome(self, status, reason, txn=None):
        value = {"txn": txn, "status": status, "reason": reason,
                 "readback": "readable_fields_match"}
        self.last_outcome = value
        return copy.deepcopy(value)

    def recovery_to(self, target, reason, release=False):
        txn = self.pending["txn"] if self.pending else None
        self.pending = None
        self.effect = self.effect_seed = self.effect_start = self.effect_frame = None
        self.confirmed = copy.deepcopy(target)
        self.observed = observed(target)
        self.observation_source = "operation_readback"
        self.recovery = "readable_fields_match"
        self.recovery_attempt += 1
        self.confirmation_source = None
        self.error = None
        self.dirty = True
        self.may_have_written = True
        if release:
            self.owner = None
            self.lease = None
        return self.outcome("recovery_needs_confirmation", reason, txn)

    def tick(self, now):
        # Lease prioriteres slik at samtidige frister ikke gir to tilbakeføringer.
        if self.owner is not None and now >= self.lease:
            return self.recovery_to(BASELINE, "release", release=True)
        if self.pending and now >= self.pending["deadlineMs"]:
            return self.recovery_to(self.pending["before"], "timeout")
        if self.effect_start is not None and self.pending is None:
            # Kun visningsfixture. Aldri IO eller en produksjonsalgoritme.
            frame = max(0, now - self.effect_start) // 250
            if frame != self.effect_frame:
                self.effect_frame = frame
                target = effect_frame(self.effect, self.effect_seed, frame * 250)
                self.observed = observed(target)
                self.observation_source = "operation_readback"
        return None

    def disconnect(self, sid):
        if sid == self.owner:
            return self.recovery_to(BASELINE, "disconnect", release=True)
        return None

    def require_owner(self, sid, allow_unowned=False):
        if self.owner == sid or (allow_unowned and self.owner is None):
            return
        raise RpcError("not_owner", "Lysstyringen tilhører en annen forbindelse.")

    def handle(self, sid, op, args, now):
        shapes = {
            "rgb_status": (),
            "rgb_apply": ("gpu", "uuid", "request", "confirmTimeoutS"),
            "rgb_confirm": ("gpu", "uuid", "txn"),
            "rgb_revert": ("gpu", "uuid", "txn"),
            "rgb_reset": ("gpu", "uuid"),
            "rgb_read": ("gpu", "uuid"),
            "rgb_release": ("gpu", "uuid"),
            "rgb_heartbeat": ("gpu", "uuid"),
            "rgb_ack_recovery": ("gpu", "uuid", "confirmedStatic", "expected", "recoveryId"),
        }
        if op not in shapes:
            raise RpcError("unknown_op", "Ukjent operasjon i RGB-fixturen.")
        fields(args, shapes[op], ("effect",) if op == "rgb_apply" else ())
        if op == "rgb_status":
            return self.status()
        if args["gpu"] != BINDING["pci"] or args["uuid"] != BINDING["uuid"]:
            raise RpcError("unknown_gpu", "Forespørselen gjelder ikke det simulerte kortet.")
        request = None
        effect = None
        if op == "rgb_apply":
            request = color_request(args["request"])
            if "effect" in args:
                effect = args["effect"]
                if effect not in SOFTWARE_EFFECTS or request["mode"] != "static":
                    raise RpcError("invalid_argument", "Effekt trenger et støttet navn og statisk startfarge.")
            if not integer(args["confirmTimeoutS"], 10, 60):
                raise RpcError("invalid_argument", "Bekreftelsesfristen må være 10–60 s.")
        if op in ("rgb_confirm", "rgb_revert") and not integer(args["txn"], 1, 2**64 - 1):
            raise RpcError("invalid_argument", "Ugyldig transaksjonsnummer.")
        if op == "rgb_ack_recovery":
            color_request(args["expected"])
            if args["confirmedStatic"] is not True:
                raise RpcError("invalid_argument", "Visuell bekreftelse mangler.")
        self.tick(now)
        result = None
        if op == "rgb_apply":
            self.require_owner(sid, allow_unowned=True)
            if self.pending or self.effect is not None:
                raise RpcError("busy", "En lysprøve venter allerede.")
            if self.error or (self.recovery and self.confirmation_source is None):
                raise RpcError("write_locked", "Gjenopprettingen må bekreftes først.")
            if effect is not None and (self.dirty or self.may_have_written or self.owner is not None
                                       or self.confirmed != BASELINE):
                raise RpcError("write_locked", "Effektstart krever et rent, opprinnelig utgangspunkt.")
            self.observed = observed(self.confirmed)
            self.observation_source = "operation_readback"
            if request == self.confirmed and effect is None:
                if self.owner is not None:
                    self.lease = now + LEASE_MS
                result = self.outcome("unchanged", "apply")
            else:
                self.effect = effect
                self.effect_seed = copy.deepcopy(request) if effect is not None else None
                self.effect_start = self.effect_frame = None
                if effect is not None:
                    request = effect_frame(effect, request, 0)
                self.pending = {
                    "txn": self.sequence, "owner": sid,
                    "deadlineMs": now + args["confirmTimeoutS"] * 1000,
                    "before": copy.deepcopy(self.confirmed),
                    "beforeObserved": copy.deepcopy(self.observed),
                    "requested": request,
                }
                self.sequence += 1
                self.owner, self.lease = sid, now + LEASE_MS
                self.dirty = self.may_have_written = True
                self.recovery = self.confirmation_source = self.error = None
                self.observed = observed(request)
                result = self.outcome("pending", "apply", self.pending["txn"])
        elif op in ("rgb_confirm", "rgb_revert"):
            if not self.pending or self.pending["txn"] != args["txn"]:
                raise RpcError("txn_mismatch", "Lysprøven finnes ikke eller fristen er ute.")
            self.require_owner(sid)
            if op == "rgb_revert":
                result = self.recovery_to(self.pending["before"], "revert")
            else:
                self.confirmed = copy.deepcopy(self.pending["requested"])
                self.pending = None
                if self.effect is not None:
                    self.effect_start = now
                    self.effect_frame = 0
                self.lease = now + LEASE_MS
                result = self.outcome("kept", "keep", args["txn"])
        elif op == "rgb_heartbeat":
            self.require_owner(sid)
            self.lease = now + LEASE_MS
        elif op == "rgb_release":
            self.require_owner(sid)
            result = self.recovery_to(BASELINE, "release", release=True)
        elif op == "rgb_reset":
            self.require_owner(sid, allow_unowned=True)
            if self.dirty:
                result = self.recovery_to(BASELINE, "reset")
            else:
                self.observed = observed(self.confirmed)
                self.observation_source = "operation_readback"
        elif op == "rgb_read":
            if self.dirty:
                raise RpcError("write_locked", "Ny feltlesing krever en ren økt.")
            self.observed = observed(self.confirmed)
            self.observation_source = "operation_readback"
        elif op == "rgb_ack_recovery":
            self.require_owner(sid, allow_unowned=True)
            if args["recoveryId"] != self.recovery_id() or args["expected"] != self.confirmed:
                raise RpcError("write_locked", "Målet er endret siden bekreftelsen ble åpnet.")
            if (not self.dirty or self.pending or self.error
                    or self.recovery != "readable_fields_match"
                    or self.confirmation_source is not None):
                raise RpcError("write_locked", "Ingen bekreftbar gjenoppretting venter.")
            self.observed = observed(self.confirmed)
            self.observation_source = "operation_readback"
            self.confirmation_source = "userVisual"
            if self.confirmed == BASELINE:
                self.dirty = self.may_have_written = False
                self.owner = self.lease = None
            else:
                self.owner, self.lease = sid, now + LEASE_MS
            result = self.outcome("user_confirmed", "user_confirmed")
        return {"outcome": result, "status": self.status()}


def safe_file(fd, mode):
    info = os.fstat(fd)
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
            or stat.S_IMODE(info.st_mode) != mode or info.st_nlink != 1):
        raise ValueError("Fixturefilen har utrygg type, eier, modus eller hardlenke.")
    return info


def fixture_root(text):
    root = Path(text)
    if (not root.is_absolute() or root == Path("/tmp")
            or root.resolve(strict=True) != root or Path("/tmp") not in root.parents):
        raise ValueError("--root må være en kanonisk, eksisterende mappe under /tmp.")
    info = root.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or stat.S_IMODE(info.st_mode) != 0o700 or os.getuid() != os.geteuid()):
        raise ValueError("Fixture-roten må være privat 0700 og eies av denne brukeren.")
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        opened = os.fstat(fd)
        if (opened.st_dev, opened.st_ino) != (info.st_dev, info.st_ino):
            raise ValueError("Fixture-roten ble byttet under åpning.")
        marker = os.open("rgb-flow.fake", os.O_RDONLY | os.O_NONBLOCK
                         | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=fd)
        try:
            info = safe_file(marker, 0o600)
            if info.st_size != len(MARKER) + 1 or os.read(marker, 128) != (MARKER + "\n").encode():
                raise ValueError("Den eksplisitte RGB-fixturemarkøren mangler.")
        finally:
            os.close(marker)
        return root, fd
    except BaseException:
        os.close(fd)
        raise


def subdirectory(parent, name):
    try:
        os.mkdir(name, 0o700, dir_fd=parent)
    except FileExistsError:
        pass
    fd = os.open(name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC, dir_fd=parent)
    info = os.fstat(fd)
    if info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) & 0o022:
        os.close(fd)
        raise ValueError("Fixture-socketmappen er utrygg.")
    return fd


class Trace:
    def __init__(self, root_fd):
        self.fd = os.open("rgb-flow.trace", os.O_WRONLY | os.O_CREAT | os.O_APPEND
                          | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600, dir_fd=root_fd)
        try:
            self.size = safe_file(self.fd, 0o600).st_size
            if self.size > MAX_TRACE:
                raise ValueError("Fixture-sporfilen er allerede for stor.")
        except BaseException:
            os.close(self.fd)
            raise

    def log(self, event, sid, state, **values):
        line = (json.dumps({"event": event, "connection": sid, "atMs": now_ms(),
                            "testMarker": MARKER, **values, "snapshot": state.status()},
                           ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        if self.size + len(line) > MAX_TRACE:
            raise ValueError("Fixture-sporfilen nådde størrelsesgrensen.")
        remaining = memoryview(line)
        while remaining:
            written = os.write(self.fd, remaining)
            remaining = remaining[written:]
        self.size += len(line)


class Client:
    def __init__(self, stream, sid):
        self.stream, self.sid = stream, sid
        self.input, self.output = bytearray(), bytearray()


class Server:
    def __init__(self, listener, trace, build):
        self.listener, self.trace = listener, trace
        self.state = FakeState(build)
        self.selector = selectors.DefaultSelector()
        self.selector.register(listener, selectors.EVENT_READ, None)
        self.clients = {}
        self.next_sid = 1
        self.stopping = False

    def close(self, client):
        self.clients.pop(client.sid, None)
        try:
            self.selector.unregister(client.stream)
        except KeyError:
            pass
        client.stream.close()
        outcome = self.state.disconnect(client.sid)
        self.trace.log("disconnect", client.sid, self.state, outcome=outcome)

    def frame(self, client, line):
        frame, rid, op = None, None, None
        logged = False
        try:
            try:
                frame = parse_json(line)
            except (ValueError, UnicodeDecodeError, RecursionError):
                raise RpcError("bad_json", "Ugyldig JSON i fixtureforespørselen.") from None
            self.trace.log("request", client.sid, self.state, request=frame,
                           op=frame.get("op") if type(frame) is dict else None)
            logged = True
            fields(frame, ("id", "op"), ("args",))
            rid, op = frame["id"], frame["op"]
            if not (integer(rid, 0, 2**64 - 1)
                    or (type(rid) is str and 1 <= len(rid) <= 64)):
                rid = None
                raise RpcError("bad_request", "Ugyldig forespørsels-id.")
            if type(op) is not str or not 1 <= len(op) <= 64:
                raise RpcError("bad_request", "Ugyldig operasjonsnavn.")
            result = self.state.handle(client.sid, op, frame.get("args", {}), now_ms())
            response = {"id": rid, "ok": True, "result": result}
        except RpcError as error:
            if not logged:
                self.trace.log("request", client.sid, self.state, request=frame,
                               malformedBytes=len(line))
            response = {"id": rid, "ok": False, "error": {
                "code": error.code, "message": str(error),
                "details": {"status": self.state.status()},
            }}
        self.trace.log("result", client.sid, self.state, op=op, response=response)
        data = (json.dumps(response, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
        if len(client.output) + len(data) > MAX_OUTPUT:
            return False
        client.output.extend(data)
        return True

    def read(self, client):
        try:
            chunk = client.stream.recv(16_384)
        except BlockingIOError:
            return True
        except OSError:
            return False
        if not chunk:
            return False
        client.input.extend(chunk)
        if len(client.input) > MAX_LINE:
            return False
        return self.frames(client)

    def frames(self, client):
        for _ in range(8):
            end = client.input.find(b"\n")
            if end < 0:
                break
            line = bytes(client.input[:end])
            del client.input[:end + 1]
            if not self.frame(client, line):
                return False
        return True

    def run(self):
        self.trace.log("started", None, self.state)
        try:
            while not self.stopping:
                outcome = self.state.tick(now_ms())
                if outcome:
                    self.trace.log("deadline", None, self.state, outcome=outcome)
                for client in list(self.clients.values()):
                    if not self.frames(client):
                        self.close(client)
                    else:
                        flags = selectors.EVENT_READ | (selectors.EVENT_WRITE if client.output else 0)
                        self.selector.modify(client.stream, flags, client)
                for key, mask in self.selector.select(0.1):
                    client = key.data
                    if client is None:
                        try:
                            stream, _ = self.listener.accept()
                        except BlockingIOError:
                            continue
                        peer = struct.unpack("3i", stream.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("3i")))
                        if peer[1] != os.getuid() or len(self.clients) >= MAX_CLIENTS:
                            stream.close()
                            continue
                        stream.setblocking(False)
                        client = Client(stream, self.next_sid)
                        self.next_sid += 1
                        self.clients[client.sid] = client
                        self.selector.register(stream, selectors.EVENT_READ, client)
                        continue
                    alive = not (mask & selectors.EVENT_READ) or self.read(client)
                    if alive and mask & selectors.EVENT_WRITE and client.output:
                        try:
                            written = client.stream.send(client.output)
                            del client.output[:written]
                            if written == 0:
                                alive = False
                        except BlockingIOError:
                            pass
                        except OSError:
                            alive = False
                    if not alive:
                        self.close(client)
        finally:
            for client in list(self.clients.values()):
                self.close(client)
            self.trace.log("stopped", None, self.state)
            self.selector.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True)
    parser.add_argument("--build", required=True, type=int)
    args = parser.parse_args()
    if not 1 <= args.build <= 2**32 - 1:
        parser.error("--build må være et positivt 32-bits byggnummer")
    root, root_fd = fixture_root(args.root)
    descriptors, listener, trace, socket_inode = [root_fd], None, None, None
    try:
        run_fd = subdirectory(root_fd, "run")
        descriptors.append(run_fd)
        socket_fd = subdirectory(run_fd, "agentic-rigg-rgb")
        descriptors.append(socket_fd)
        trace = Trace(root_fd)
        # Relativ bind under allerede åpnet, kontrollert mappe. Ingen /proc-
        # omvei og ingen fjerning av en socket denne prosessen ikke opprettet.
        os.fchdir(socket_fd)
        listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        previous_umask = os.umask(0o177)
        try:
            listener.bind("helper.sock")
        finally:
            os.umask(previous_umask)
        metadata = os.stat("helper.sock", dir_fd=socket_fd, follow_symlinks=False)
        socket_inode = (metadata.st_dev, metadata.st_ino)
        listener.listen(MAX_CLIENTS)
        listener.setblocking(False)
        server = Server(listener, trace, args.build)

        def stop(_signal, _frame):
            server.stopping = True
        signal.signal(signal.SIGTERM, stop)
        signal.signal(signal.SIGINT, stop)
        print(json.dumps({"testMarker": MARKER, "root": str(root),
                          "socket": str(root / "run/agentic-rigg-rgb/helper.sock")}), flush=True)
        server.run()
    finally:
        if listener is not None:
            listener.close()
        if socket_inode is not None:
            try:
                metadata = os.stat("helper.sock", dir_fd=socket_fd, follow_symlinks=False)
                if (metadata.st_dev, metadata.st_ino) == socket_inode:
                    os.unlink("helper.sock", dir_fd=socket_fd)
            except FileNotFoundError:
                pass
        if trace is not None:
            os.close(trace.fd)
        for fd in reversed(descriptors):
            os.close(fd)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        print(f"falsk-rgb: {error}", file=sys.stderr)
        raise SystemExit(2) from None
