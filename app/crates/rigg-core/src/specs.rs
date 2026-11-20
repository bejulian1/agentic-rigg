//! Spesifikasjonstabellen («Referanse, ikke målt»), port av `server/specs.js`.
//!
//! Tallene er referanseverdier for brikken og referansekortet. De er **ikke**
//! målt på maskinen, og kortprodusenten kan ha endret klokker eller minne.
//! Siden og rapporten merker dem alltid som referanse.

use crate::{fmt as F, tr, trf};
use serde::Serialize;

/// Referansedata for en brikke, slått opp på PCI-ID. Ikke målt på maskinen.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GpuSpec {
    /// «10de:1e87».
    pub key: &'static str,
    pub name: &'static str,
    pub chip: &'static str,
    pub variant: &'static str,
    pub architecture: &'static str,
    pub process: &'static str,
    pub die_mm2: u32,
    pub transistors_bn: f64,
    pub cuda: u32,
    pub sm: u32,
    pub tmu: u32,
    pub rop: u32,
    pub tensor: u32,
    pub rt: u32,
    pub l2_kb: u32,
    pub mem_type: &'static str,
    pub bus_width: u32,
    pub mem_gb: u32,
    pub mem_gbps: f64,
    pub base_mhz: u32,
    pub boost_mhz: u32,
    pub tdp_w: u32,
    pub launched: &'static str,
    pub bus: &'static str,
    pub directx: &'static str,
    pub vulkan: &'static str,
    pub opengl: &'static str,
    pub mem_vendor_note: &'static str,
}

/// Ett avledet tall med formelen i klartekst (verktøytipset).
#[derive(Clone, Debug, PartialEq)]
pub struct Derived {
    pub value: f64,
    pub unit: &'static str,
    pub formula: String,
}

const MEM_NOTE: &str = "Minneleverandøren (Samsung eller Micron) står i VBIOS-en. NVML oppgir den ikke, og VBIOS kan ikke leses uten root.";

const TU104_2080: GpuSpec = GpuSpec {
    key: "10de:1e87",
    name: "GeForce RTX 2080 Rev. A",
    chip: "TU104",
    variant: "TU104-400A-A1",
    architecture: "Turing",
    process: "12 nm (TSMC 12FFN)",
    die_mm2: 545,
    transistors_bn: 13.6,
    cuda: 2944,
    sm: 46,
    tmu: 184,
    rop: 64,
    tensor: 368,
    rt: 46,
    l2_kb: 4096,
    mem_type: "GDDR6",
    bus_width: 256,
    mem_gb: 8,
    mem_gbps: 14.0,
    base_mhz: 1515,
    boost_mhz: 1710,
    tdp_w: 215,
    launched: "20. september 2018",
    bus: "PCIe 3.0 x16",
    directx: "12 Ultimate (12_2)",
    vulkan: "1.4",
    opengl: "4.6",
    mem_vendor_note: MEM_NOTE,
};

/// Tabellen. Nøkkelen er «leverandør:enhet» fra PCI-ID-en.
pub const TABLE: &[GpuSpec] = &[
    TU104_2080,
    GpuSpec { key: "10de:1e82", name: "GeForce RTX 2080", variant: "TU104-400-A1", ..TU104_2080 },
    GpuSpec {
        key: "10de:1e81",
        name: "GeForce RTX 2080 SUPER",
        variant: "TU104-450-A1",
        cuda: 3072,
        sm: 48,
        tmu: 192,
        tensor: 384,
        rt: 48,
        mem_gbps: 15.5,
        base_mhz: 1650,
        boost_mhz: 1815,
        tdp_w: 250,
        launched: "23. juli 2019",
        ..TU104_2080
    },
    GpuSpec {
        key: "10de:1e84",
        name: "GeForce RTX 2070 SUPER",
        variant: "TU104-410-A1",
        cuda: 2560,
        sm: 40,
        tmu: 160,
        tensor: 320,
        rt: 40,
        base_mhz: 1605,
        boost_mhz: 1770,
        tdp_w: 215,
        launched: "9. juli 2019",
        ..TU104_2080
    },
];

/// Slår opp et kort på PCI-enhets-ID slik NVML gir den (`0x1e8710de`:
/// enhet i de øverste 16 bitene, leverandør i de nederste). Subsystemet
/// brukes ikke ennå: Palit-kortet melder NVIDIAs egen ID. Gir `None` når
/// kortet ikke er i tabellen.
pub fn lookup(pci_device: u32, _pci_subsystem: Option<u32>) -> Option<GpuSpec> {
    let key = format!("{:04x}:{:04x}", pci_device & 0xffff, pci_device >> 16);
    lookup_key(&key)
}

/// Slår opp på «10de:1e87» (små eller store bokstaver).
pub fn lookup_key(key: &str) -> Option<GpuSpec> {
    let k = key.to_ascii_lowercase();
    TABLE.iter().find(|s| s.key == k).cloned()
}

impl GpuSpec {
    pub fn mem_vendor_note_text(&self) -> &'static str {
        if self.mem_vendor_note == MEM_NOTE {
            tr!("Minneleverandøren (Samsung eller Micron) står i VBIOS-en. NVML oppgir den ikke, og VBIOS kan ikke leses uten root.")
        } else {
            self.mem_vendor_note
        }
    }

    pub fn launched_text(&self) -> String {
        match self.launched {
            "20. september 2018" => F::date(2018, 9, 20, false),
            "23. juli 2019" => F::date(2019, 7, 23, false),
            "9. juli 2019" => F::date(2019, 7, 9, false),
            other => other.to_string(),
        }
    }

    pub fn pixel_fill(&self) -> Derived {
        let v = self.rop as f64 * self.boost_mhz as f64 / 1000.0;
        Derived {
            value: (v * 10.0).round() / 10.0,
            unit: "GPixel/s",
            formula: trf!("{rop} ROP × {mhz} MHz (referanse-boost) = {verdi} GPixel/s", rop = self.rop, mhz = self.boost_mhz, verdi = F::num(Some(v), 1)),
        }
    }

    pub fn texture_fill(&self) -> Derived {
        let v = self.tmu as f64 * self.boost_mhz as f64 / 1000.0;
        Derived {
            value: (v * 10.0).round() / 10.0,
            unit: "GTexel/s",
            formula: trf!("{tmu} TMU × {mhz} MHz (referanse-boost) = {verdi} GTexel/s", tmu = self.tmu, mhz = self.boost_mhz, verdi = F::num(Some(v), 1)),
        }
    }

    pub fn fp32(&self) -> Derived {
        let v = self.cuda as f64 * 2.0 * self.boost_mhz as f64 / 1e6;
        Derived {
            value: (v * 100.0).round() / 100.0,
            unit: "TFLOPS",
            formula: trf!("{cuda} CUDA-kjerner × 2 operasjoner (FMA) × {mhz} MHz = {verdi} TFLOPS", cuda = self.cuda, mhz = self.boost_mhz, verdi = F::num(Some(v), 2)),
        }
    }

    pub fn bandwidth(&self) -> Derived {
        let v = self.bus_width as f64 / 8.0 * self.mem_gbps;
        let gbps = F::num(Some(self.mem_gbps), if self.mem_gbps.fract() == 0.0 { 0 } else { 1 });
        Derived {
            value: v,
            unit: "GB/s",
            formula: trf!("{bredde} bit ÷ 8 × {gbps} Gbps = {verdi} GB/s", bredde = self.bus_width, gbps = gbps, verdi = F::num(Some(v), 0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oppslag_på_nvml_id() {
        let s = lookup(0x1e87_10de, Some(0x1e87_10de)).unwrap();
        assert_eq!(s.name, "GeForce RTX 2080 Rev. A");
        assert_eq!(s.cuda, 2944);
        assert_eq!(lookup_key("10DE:1E82").unwrap().variant, "TU104-400-A1");
        assert!(lookup(0x1234_10de, None).is_none());
    }

    #[test]
    fn avledede_tall_som_demoen() {
        let s = lookup_key("10de:1e87").unwrap();
        assert_eq!(s.pixel_fill().value, 109.4);
        assert_eq!(s.texture_fill().value, 314.6);
        assert_eq!(s.fp32().value, 10.07);
        assert_eq!(s.bandwidth().value, 448.0);
        assert_eq!(s.bandwidth().formula, "256 bit ÷ 8 × 14 Gbps = 448 GB/s");
        assert!(s.fp32().formula.contains("10,07 TFLOPS"));
    }
}
