// Bare for den private, hodeløse testøkta i runde 6B (skallet). Virtuell
// peker og tastatur i Clutter gir ekte hover, trykk og tastetrykk gjennom
// mutter og Wayland, og Shell tar skjermbilder av hele skjermen eller et utsnitt.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const XML = `<node><interface name="org.rigg6b.Skall">
  <method name="Shot"><arg type="s" direction="in" name="path"/></method>
  <method name="ShotArea"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
  <method name="Move"><arg type="d" direction="in"/><arg type="d" direction="in"/></method>
  <method name="Button"><arg type="u" direction="in" name="state"/></method>
  <method name="Key"><arg type="u" direction="in" name="keyval"/><arg type="u" direction="in" name="mods"/></method>
  <method name="HideOverview"/>
  <method name="Windows"><arg type="s" direction="out"/></method>
  <method name="Place"><arg type="s" direction="in" name="wmclass"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="Raise"><arg type="s" direction="in" name="wmclass"/></method>
  <method name="Close"><arg type="s" direction="in" name="wmclass"/></method>
</interface></node>`;

const MODS = [[1, Clutter.KEY_Control_L], [2, Clutter.KEY_Shift_L], [4, Clutter.KEY_Alt_L]];

function find(wmclass) {
    return global.get_window_actors().map(a => a.meta_window).filter(w => `${w.get_wm_class()}`.includes(wmclass) || `${w.get_title()}`.includes(wmclass));
}

export default class Skall extends Extension {
    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this._impl.export(Gio.DBus.session, '/org/rigg6b/Skall');
        this._own = Gio.bus_own_name(Gio.BusType.SESSION, 'org.rigg6b.Skall', 0, null, null, null);
        const seat = Clutter.get_default_backend().get_default_seat();
        this._kbd = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._ptr = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._t = GLib.get_monotonic_time();
    }

    disable() {
        this._impl?.unexport();
        if (this._own)
            Gio.bus_unown_name(this._own);
        this._impl = null;
        this._kbd = null;
        this._ptr = null;
    }

    _tick() {
        this._t = Math.max(this._t + 2000, GLib.get_monotonic_time());
        return this._t;
    }

    Move(x, y) {
        this._ptr.notify_absolute_motion(this._tick(), x, y);
    }

    // 1 = trykk, 0 = slipp (venstre knapp).
    Button(state) {
        this._ptr.notify_button(this._tick(), Clutter.BUTTON_PRIMARY, state ? Clutter.ButtonState.PRESSED : Clutter.ButtonState.RELEASED);
    }

    Key(keyval, mods) {
        for (const [bit, k] of MODS) {
            if (mods & bit)
                this._kbd.notify_keyval(this._tick(), k, Clutter.KeyState.PRESSED);
        }
        this._kbd.notify_keyval(this._tick(), keyval, Clutter.KeyState.PRESSED);
        this._kbd.notify_keyval(this._tick(), keyval, Clutter.KeyState.RELEASED);
        for (const [bit, k] of MODS.slice().reverse()) {
            if (mods & bit)
                this._kbd.notify_keyval(this._tick(), k, Clutter.KeyState.RELEASED);
        }
    }

    HideOverview() {
        Main.overview.hide();
    }

    // Vinduene som JSON: klasse, tittel, ramme og bufferramme (logiske piksler).
    Windows() {
        const out = global.get_window_actors().map(a => {
            const w = a.meta_window;
            const f = w.get_frame_rect();
            const b = w.get_buffer_rect();
            return {cls: w.get_wm_class(), title: w.get_title(), frame: [f.x, f.y, f.width, f.height], buffer: [b.x, b.y, b.width, b.height], focus: w.has_focus()};
        });
        return JSON.stringify({scale: global.display.get_monitor_scale(0), windows: out});
    }

    Place(wmclass, x, y, w, h) {
        for (const win of find(wmclass)) {
            try {
                win.unmaximize();
            } catch (e) {
                win.unmaximize(3);
            }
            win.move_resize_frame(true, x, y, w, h);
        }
    }

    Raise(wmclass) {
        for (const win of find(wmclass))
            win.activate(global.get_current_time());
    }

    Close(wmclass) {
        for (const win of find(wmclass))
            win.delete(global.get_current_time());
    }

    ShotAsync([path], inv) {
        const s = new Shell.Screenshot();
        const file = Gio.File.new_for_path(path);
        const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
        s.screenshot(false, stream, (o, res) => {
            try {
                o.screenshot_finish(res);
                stream.close(null);
                inv.return_value(null);
            } catch (e) {
                inv.return_dbus_error('org.rigg6b.Feil', String(e));
            }
        });
    }

    ShotAreaAsync([x, y, w, h, path], inv) {
        const s = new Shell.Screenshot();
        const file = Gio.File.new_for_path(path);
        const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
        s.screenshot_area(x, y, w, h, stream, (o, res) => {
            try {
                o.screenshot_area_finish(res);
                stream.close(null);
                inv.return_value(null);
            } catch (e) {
                inv.return_dbus_error('org.rigg6b.Feil', String(e));
            }
        });
    }
}
