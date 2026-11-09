// Bare for den private, hodeløse testøkta i runde 4B (tastaturrunden).
// Et virtuelt tastatur i Clutter gir ekte tastetrykk gjennom mutter og
// Wayland til vinduet, akkurat som et fysisk tastatur. Pluss skjermbilder.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const XML = `<node><interface name="org.rigg4b.Tast">
  <method name="Shot"><arg type="s" direction="in" name="path"/></method>
  <method name="Key"><arg type="u" direction="in" name="keyval"/><arg type="u" direction="in" name="mods"/></method>
  <method name="Type"><arg type="s" direction="in" name="text"/></method>
  <method name="HideOverview"/>
  <method name="Focus"><arg type="s" direction="out"/></method>
</interface></node>`;

// mods: 1 = Ctrl, 2 = Shift, 4 = Alt
const MODS = [[1, Clutter.KEY_Control_L], [2, Clutter.KEY_Shift_L], [4, Clutter.KEY_Alt_L]];

export default class Tast extends Extension {
    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this._impl.export(Gio.DBus.session, '/org/rigg4b/Tast');
        this._own = Gio.bus_own_name(Gio.BusType.SESSION, 'org.rigg4b.Tast', 0, null, null, null);
        this._kbd = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._t = GLib.get_monotonic_time();
    }

    disable() {
        this._impl?.unexport();
        if (this._own)
            Gio.bus_unown_name(this._own);
        this._impl = null;
        this._kbd = null;
    }

    _tick() {
        this._t = Math.max(this._t + 2000, GLib.get_monotonic_time());
        return this._t;
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

    Type(text) {
        for (const ch of text)
            this.Key(Clutter.unicode_to_keysym(ch.codePointAt(0)), 0);
    }

    HideOverview() {
        Main.overview.hide();
    }

    Focus() {
        const w = global.display.focus_window;
        return w ? `${w.get_wm_class()} · ${w.get_title()}` : '';
    }

    ShotAsync([path], inv) {
        const file = Gio.File.new_for_path(path);
        const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
        new Shell.Screenshot().screenshot(false, stream, (o, res) => {
            try {
                o.screenshot_finish(res);
                stream.close(null);
                inv.return_value(null);
            } catch (e) {
                inv.return_dbus_error('org.rigg4b.Feil', String(e));
            }
        });
    }
}
