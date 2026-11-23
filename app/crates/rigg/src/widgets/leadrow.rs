//! En rad der det første barnet går foran: overskriften får sin naturlige
//! bredde, og merket etter den får det som er igjen (og kutter teksten sin
//! med «…» når det er for lite).
//!
//! En `GtkBox` deler plassen jevnt mellom barn som kan krympe, så både
//! overskriften og merket ble kuttet. Før dette sto merket på en egen linje
//! når det ikke fikk plass («Grafikkortsstyrning» på svensk).

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::Cell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LeadRow {
        pub gap: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LeadRow {
        const NAME: &'static str = "RiggLeadRow";
        type Type = super::LeadRow;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LeadRow {
        fn dispose(&self) {
            while let Some(c) = self.obj().first_child() {
                c.unparent();
            }
        }
    }

    impl LeadRow {
        /// Overskriften og merket (når det finnes og vises).
        fn kids(&self) -> Option<(gtk::Widget, Option<gtk::Widget>)> {
            let lead = self.obj().first_child()?;
            let trail = lead.next_sibling().filter(|t| t.is_visible());
            Some((lead, trail))
        }
    }

    impl WidgetImpl for LeadRow {
        fn measure(&self, o: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let Some((lead, trail)) = self.kids() else { return (0, 0, -1, -1) };
            let (lmin, lnat, _, _) = lead.measure(o, -1);
            let Some(trail) = trail else { return (lmin, lnat, -1, -1) };
            let (tmin, tnat, _, _) = trail.measure(o, -1);
            match o {
                gtk::Orientation::Horizontal => (lmin + self.gap.get() + tmin, lnat + self.gap.get() + tnat, -1, -1),
                _ => (lmin.max(tmin), lnat.max(tnat), -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let Some((lead, trail)) = self.kids() else { return };
            let Some(trail) = trail else {
                lead.size_allocate(&gtk::Allocation::new(0, 0, width, height), -1);
                return;
            };
            let (_, lnat, _, _) = lead.measure(gtk::Orientation::Horizontal, -1);
            let (tmin, tnat, _, _) = trail.measure(gtk::Orientation::Horizontal, -1);
            let (lw, tw) = super::split(width, self.gap.get(), lnat, (tmin, tnat));
            let rtl = self.obj().direction() == gtk::TextDirection::Rtl;
            lead.size_allocate(&gtk::Allocation::new(if rtl { width - lw } else { 0 }, 0, lw, height), -1);
            trail.size_allocate(&gtk::Allocation::new(if rtl { 0 } else { width - tw }, 0, tw, height), -1);
        }
    }
}

/// (bredde til overskriften, bredde til merket): overskriften får sin
/// naturlige bredde først, merket resten, men aldri under sin minste bredde.
pub fn split(width: i32, gap: i32, lead_nat: i32, trail: (i32, i32)) -> (i32, i32) {
    let (tmin, tnat) = (trail.0, trail.1.max(trail.0));
    let tw = (width - gap - lead_nat).clamp(tmin, tnat);
    ((width - gap - tw).max(1), tw)
}

glib::wrapper! {
    pub struct LeadRow(ObjectSubclass<imp::LeadRow>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LeadRow {
    pub fn new(lead: &impl IsA<gtk::Widget>, gap: i32) -> LeadRow {
        let r: LeadRow = glib::Object::new();
        r.imp().gap.set(gap);
        lead.set_parent(&r);
        r
    }

    /// Merket etter overskriften.
    pub fn trail(&self) -> Option<gtk::Widget> {
        self.first_child().and_then(|l| l.next_sibling())
    }

    /// Bytter merket (eller fjerner det).
    pub fn set_trail(&self, trail: Option<&impl IsA<gtk::Widget>>) {
        if let Some(old) = self.trail() {
            old.unparent();
        }
        if let Some(t) = trail {
            t.set_parent(self);
        }
        self.queue_resize();
    }
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn overskriften_går_foran_merket() {
        // Bokmål: «Skjermkortstyring» (101) og «Simulert kort» (99) får plass i 204 px.
        assert_eq!(split(204, 4, 101, (40, 99)), (101, 99));
        // Svensk: «Grafikkortsstyrning» (125). Merket får resten og kutter teksten.
        assert_eq!(split(204, 4, 125, (40, 105)), (125, 75));
        // Ikke plass til begge: merket står på sin minste bredde, overskriften kuttes.
        assert_eq!(split(150, 4, 125, (40, 105)), (106, 40));
        // God plass: merket får bare sin naturlige bredde og står mot høyre kant.
        assert_eq!(split(400, 4, 101, (40, 99)), (297, 99));
        assert_eq!(split(10, 4, 125, (40, 30)), (1, 40), "aldri null eller negativ bredde");
    }
}
