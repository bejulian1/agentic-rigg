#!/usr/bin/python3 -I
"""Attrapp av org.freedesktop.systemd1.Manager for den PRIVATE testøkta (runde 3B).

Bare GetUnitFileState, EnableUnitFiles, DisableUnitFiles og Reload, med
samme filvirkning som systemd --user: en symlenke i
$XDG_CONFIG_HOME/systemd/user/<WantedBy>.wants/. Nekter å starte hvis
XDG_CONFIG_HOME peker på brukerens ekte ~/.config, eller hvis navnet
allerede finnes på bussen (da er dette ikke en privat buss).
"""
import os, sys, configparser
from gi.repository import Gio, GLib

home_real = os.path.expanduser("~julian")
cfg = os.environ.get("XDG_CONFIG_HOME", "")
if not cfg or os.path.realpath(cfg) == os.path.realpath(os.path.join(home_real, ".config")):
    sys.exit("XDG_CONFIG_HOME er ikke privat; nekter")
UNIT_DIR = os.path.join(cfg, "systemd", "user")
LOG = open(os.environ.get("ATTRAPP_LOGG", "/dev/stderr"), "a", buffering=1)

XML = """<node><interface name="org.freedesktop.systemd1.Manager">
<method name="GetUnitFileState"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
<method name="EnableUnitFiles"><arg type="as" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="out"/><arg type="a(sss)" direction="out"/></method>
<method name="DisableUnitFiles"><arg type="as" direction="in"/><arg type="b" direction="in"/><arg type="a(sss)" direction="out"/></method>
<method name="Reload"/>
</interface></node>"""

def unit_path(name):
    p = os.path.join(UNIT_DIR, name)
    return p if os.path.isfile(p) else None

def wanted_by(path):
    c = configparser.ConfigParser(strict=False, interpolation=None)
    c.read(path)
    return c.get("Install", "WantedBy", fallback="").split()

def links(name):
    out = []
    if os.path.isdir(UNIT_DIR):
        for d in os.listdir(UNIT_DIR):
            p = os.path.join(UNIT_DIR, d, name)
            if d.endswith(".wants") and os.path.islink(p):
                out.append(p)
    return out

def call(conn, sender, path, iface, method, params, inv):
    args = params.unpack()
    LOG.write(f"{method} {args}\n")
    try:
        if method == "GetUnitFileState":
            p = unit_path(args[0])
            if not p:
                return inv.return_dbus_error("org.freedesktop.DBus.Error.FileNotFound", f"Unit file {args[0]} does not exist.")
            st = "enabled" if links(args[0]) else ("disabled" if wanted_by(p) else "static")
            return inv.return_value(GLib.Variant("(s)", (st,)))
        if method == "EnableUnitFiles":
            changes = []
            for name in args[0]:
                p = unit_path(name)
                if not p:
                    return inv.return_dbus_error("org.freedesktop.DBus.Error.FileNotFound", f"Unit file {name} does not exist.")
                for t in wanted_by(p):
                    d = os.path.join(UNIT_DIR, t + ".wants")
                    os.makedirs(d, exist_ok=True)
                    link = os.path.join(d, name)
                    if not os.path.islink(link):
                        os.symlink(p, link)
                        changes.append(("symlink", link, p))
            return inv.return_value(GLib.Variant("(ba(sss))", (True, changes)))
        if method == "DisableUnitFiles":
            changes = []
            for name in args[0]:
                for link in links(name):
                    os.unlink(link)
                    changes.append(("unlink", link, ""))
            return inv.return_value(GLib.Variant("(a(sss))", (changes,)))
        if method == "Reload":
            return inv.return_value(None)
    except Exception as e:  # noqa
        inv.return_dbus_error("org.freedesktop.DBus.Error.Failed", str(e))

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
has = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner",
                    GLib.Variant("(s)", ("org.freedesktop.systemd1",)), None, 0, -1, None).unpack()[0]
if has:
    sys.exit("org.freedesktop.systemd1 finnes allerede på denne bussen; nekter")
node = Gio.DBusNodeInfo.new_for_xml(XML)
bus.register_object("/org/freedesktop/systemd1", node.interfaces[0], call, None, None)
Gio.bus_own_name_on_connection(bus, "org.freedesktop.systemd1", Gio.BusNameOwnerFlags.DO_NOT_QUEUE, None, None)
LOG.write(f"attrapp klar, enhetsmappe {UNIT_DIR}\n")
GLib.MainLoop().run()
