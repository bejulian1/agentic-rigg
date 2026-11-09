// Bare for den private, hodeløse testøkta i runde 3B. Gir et lite D-Bus-
// grensesnitt for skjermbilder, panelets indikatorer og klikk med en
// virtuell mus.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const XML = `<node><interface name="org.rigg3b.Verktoy">
  <method name="Shot"><arg type="s" direction="in" name="path"/></method>
  <method name="Area"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
  <method name="Indicators"><arg type="s" direction="out"/></method>
  <method name="Menu"><arg type="s" direction="in" name="key"/><arg type="b" direction="in" name="open"/><arg type="s" direction="out"/></method>
  <method name="Click"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in" name="button"/></method>
  <method name="Move"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="HideOverview"/>
</interface></node>`;

function geom(a) {
    const [x, y] = a.get_transformed_position();
    const [w, h] = a.get_transformed_size();
    return `${Math.round(x)} ${Math.round(y)} ${Math.round(w)} ${Math.round(h)}`;
}

export default class Verktoy extends Extension {
    enable() {
        this._impl = Gio.DBusExportedObject.wrapJSObject(XML, this);
        this._impl.export(Gio.DBus.session, '/org/rigg3b/Verktoy');
        this._own = Gio.bus_own_name(Gio.BusType.SESSION, 'org.rigg3b.Verktoy', 0, null, null, null);
        this._ptr = Clutter.get_default_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    }

    disable() {
        this._impl?.unexport();
        if (this._own)
            Gio.bus_unown_name(this._own);
        this._impl = null;
        this._ptr = null;
    }

    _write(path, fn, inv) {
        const file = Gio.File.new_for_path(path);
        const stream = file.replace(null, false, Gio.FileCreateFlags.NONE, null);
        const shooter = new Shell.Screenshot();
        fn(shooter, stream, (o, res) => {
            try {
                if (fn === this._area)
                    o.screenshot_area_finish(res);
                else
                    o.screenshot_finish(res);
                stream.close(null);
                inv.return_value(null);
            } catch (e) {
                inv.return_dbus_error('org.rigg3b.Feil', String(e));
            }
        });
    }

    ShotAsync([path], inv) {
        this._write(path, (s, st, cb) => s.screenshot(false, st, cb), inv);
    }

    AreaAsync([x, y, w, h, path], inv) {
        this._area = (s, st, cb) => s.screenshot_area(x, y, w, h, st, cb);
        this._write(path, this._area, inv);
    }

    Indicators() {
        const out = [];
        for (const [k, v] of Object.entries(Main.panel.statusArea)) {
            if (!v)
                continue;
            const c = v.container ?? v;
            out.push(`${k} ${geom(c)} ${c.visible && v.visible}`);
        }
        return out.join('\n');
    }

    Menu(key, open) {
        const ind = Main.panel.statusArea[key];
        if (!ind)
            return 'finnes ikke';
        if (open)
            ind.menu.open(false);
        else
            ind.menu.close(false);
        return geom(ind.menu.actor);
    }

    Click(x, y, button) {
        const t = GLib.get_monotonic_time();
        this._ptr.notify_absolute_motion(t, x, y);
        this._ptr.notify_button(t + 1000, button, Clutter.ButtonState.PRESSED);
        this._ptr.notify_button(t + 60000, button, Clutter.ButtonState.RELEASED);
    }

    Move(x, y) {
        this._ptr.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
    }

    HideOverview() {
        Main.overview.hide();
    }
}
