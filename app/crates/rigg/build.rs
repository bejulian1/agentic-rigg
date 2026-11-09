// Kompilerer CSS, ikonene og variantene inn i binæren (arkitektur.md 3.3,
// punkt 4): ingen filer å lete etter ved oppstart.
fn main() {
    glib_build_tools::compile_resources(&["../../data"], "../../data/rigg.gresource.xml", "rigg.gresource");
    println!("cargo:rerun-if-changed=../../data");
}
