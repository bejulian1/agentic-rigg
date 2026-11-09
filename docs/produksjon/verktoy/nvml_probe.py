#!/usr/bin/env python3
"""NVML-sonde for Agentic Rigg – BARE LESING.

Kaller kun nvmlInit_v2, nvmlShutdown, nvmlSystemGet* og nvmlDeviceGet*.
Ingen Set-, Reset-, Clear- eller Register-kall finnes i denne fila, og
funksjonen `call()` nekter å slå opp noe navn som ikke begynner med
«nvmlDeviceGet» eller «nvmlSystemGet».

Bruk:  python3 -I nvml_probe.py            (tabell)
       python3 -I nvml_probe.py --json     (maskinlesbart)

       python3 -I nvml_probe.py --sakte   (ett kall om gangen med pause, så
                                            kortet blir i P8 og tidene viser
                                            kostnaden i ro)

Hver verdi leses flere ganger, og medianen av tiden per kall skrives ut.
Merk: tette lesinger vekker kortet (P8 → P0), så standardmodusen måler
kostnaden i P0. Bruk --sakte for kostnaden i ro.
"""
import ctypes as C
import json
import statistics
import sys
import time

NVML_OK = 0
ERR = {
    0: "OK", 1: "UNINITIALIZED", 2: "INVALID_ARGUMENT", 3: "NOT_SUPPORTED",
    4: "NO_PERMISSION", 5: "ALREADY_INITIALIZED", 6: "NOT_FOUND",
    7: "INSUFFICIENT_SIZE", 8: "INSUFFICIENT_POWER", 9: "DRIVER_NOT_LOADED",
    10: "TIMEOUT", 11: "IRQ_ISSUE", 12: "LIBRARY_NOT_FOUND",
    13: "FUNCTION_NOT_FOUND", 14: "CORRUPTED_INFOROM", 15: "GPU_IS_LOST",
    16: "RESET_REQUIRED", 17: "OPERATING_SYSTEM", 18: "LIB_RM_VERSION_MISMATCH",
    19: "IN_USE", 20: "MEMORY", 21: "NO_DATA", 22: "VGPU_ECC_NOT_SUPPORTED",
    23: "INSUFFICIENT_RESOURCES", 24: "FREQ_NOT_SUPPORTED",
    25: "ARGUMENT_VERSION_MISMATCH", 26: "DEPRECATED", 27: "NOT_READY",
    28: "GPU_NOT_FOUND", 29: "INVALID_STATE", 999: "UNKNOWN",
}

t0 = time.perf_counter_ns()
lib = C.CDLL("libnvidia-ml.so.1")
t_dlopen = (time.perf_counter_ns() - t0) / 1e6

ALLOWED_PREFIX = ("nvmlDeviceGet", "nvmlSystemGet")


def fn(name):
    if not name.startswith(ALLOWED_PREFIX):
        raise RuntimeError(f"Avvist: {name} er ikke et lesekall")
    try:
        return getattr(lib, name)
    except AttributeError:
        return None


def ver(struct, v):
    """NVML_STRUCT_VERSION: sizeof(struct) | (versjon << 24)."""
    return C.sizeof(struct) | (v << 24)


# ---------------- strukturer ----------------
class PciInfo(C.Structure):
    _fields_ = [("busIdLegacy", C.c_char * 16), ("domain", C.c_uint), ("bus", C.c_uint),
                ("device", C.c_uint), ("pciDeviceId", C.c_uint), ("pciSubSystemId", C.c_uint),
                ("busId", C.c_char * 32)]


class Util(C.Structure):
    _fields_ = [("gpu", C.c_uint), ("memory", C.c_uint)]


class Mem(C.Structure):
    _fields_ = [("total", C.c_ulonglong), ("free", C.c_ulonglong), ("used", C.c_ulonglong)]


class MemV2(C.Structure):
    _fields_ = [("version", C.c_uint), ("total", C.c_ulonglong), ("reserved", C.c_ulonglong),
                ("free", C.c_ulonglong), ("used", C.c_ulonglong)]


class Bar1(C.Structure):
    _fields_ = [("bar1Total", C.c_ulonglong), ("bar1Free", C.c_ulonglong), ("bar1Used", C.c_ulonglong)]


class TempV(C.Structure):
    _fields_ = [("version", C.c_uint), ("sensorType", C.c_int), ("temperature", C.c_int)]


class Margin(C.Structure):
    _fields_ = [("version", C.c_uint), ("marginTemperature", C.c_int)]


class FanRpm(C.Structure):
    _fields_ = [("version", C.c_uint), ("fan", C.c_uint), ("speed", C.c_uint)]


class Cooler(C.Structure):
    _fields_ = [("version", C.c_uint), ("index", C.c_uint), ("signalType", C.c_int), ("target", C.c_int)]


class ClockOffset(C.Structure):
    _fields_ = [("version", C.c_uint), ("type", C.c_int), ("pstate", C.c_int),
                ("clockOffsetMHz", C.c_int), ("minClockOffsetMHz", C.c_int), ("maxClockOffsetMHz", C.c_int)]


class StrV(C.Structure):  # nvmlDevicePerfModes_t / nvmlDeviceCurrentClockFreqs_t
    _fields_ = [("version", C.c_uint), ("str", C.c_char * 2048)]


class ProcInfo(C.Structure):
    _fields_ = [("pid", C.c_uint), ("usedGpuMemory", C.c_ulonglong),
                ("gpuInstanceId", C.c_uint), ("computeInstanceId", C.c_uint)]


class ProcUtil(C.Structure):
    _fields_ = [("pid", C.c_uint), ("timeStamp", C.c_ulonglong), ("smUtil", C.c_uint),
                ("memUtil", C.c_uint), ("encUtil", C.c_uint), ("decUtil", C.c_uint)]


class Violation(C.Structure):
    _fields_ = [("referenceTime", C.c_ulonglong), ("violationTime", C.c_ulonglong)]


class DynUtil(C.Structure):
    _fields_ = [("bIsPresent", C.c_uint), ("percentage", C.c_uint),
                ("incThreshold", C.c_uint), ("decThreshold", C.c_uint)]


class DynPstates(C.Structure):
    _fields_ = [("flags", C.c_uint), ("utilization", DynUtil * 8)]


class ThermalSensor(C.Structure):
    _fields_ = [("controller", C.c_int), ("defaultMinTemp", C.c_int), ("defaultMaxTemp", C.c_int),
                ("currentTemp", C.c_int), ("target", C.c_int)]


class ThermalSettings(C.Structure):
    _fields_ = [("count", C.c_uint), ("sensor", ThermalSensor * 3)]


class FieldValue(C.Structure):
    _fields_ = [("fieldId", C.c_uint), ("scopeId", C.c_uint), ("timestamp", C.c_longlong),
                ("latencyUsec", C.c_longlong), ("valueType", C.c_int), ("nvmlReturn", C.c_int),
                ("value", C.c_ulonglong)]


# ---------------- hjelpere ----------------
results = []


SLOW = "--sakte" in sys.argv


def timed(f, args, reps):
    if SLOW:
        reps = 1
        time.sleep(0.4)
    times = []
    rc = None
    for _ in range(reps):
        a = time.perf_counter_ns()
        rc = f(*args)
        times.append((time.perf_counter_ns() - a) / 1000)
    return rc, statistics.median(times), min(times), max(times)


def call(group, label, name, args, decode, reps=30, unit=""):
    f = fn(name)
    if f is None:
        results.append(dict(group=group, label=label, fn=name, rc="FUNCTION_NOT_FOUND", value=None, us=None, unit=unit))
        return None
    rc, med, lo, hi = timed(f, args, reps)
    val = None
    if rc == NVML_OK:
        try:
            val = decode()
        except Exception as e:  # noqa: BLE001
            val = f"dekodefeil: {e}"
    results.append(dict(group=group, label=label, fn=name, rc=ERR.get(rc, str(rc)), value=val,
                        us=round(med, 1), us_min=round(lo, 1), us_max=round(hi, 1), unit=unit))
    return val


def u():
    return C.c_uint(0)


def s(n=96):
    return C.create_string_buffer(n)


# ---------------- init ----------------
lib.nvmlInit_v2.restype = C.c_int
t0 = time.perf_counter_ns()
rc = lib.nvmlInit_v2()
t_init = (time.perf_counter_ns() - t0) / 1e6
if rc != NVML_OK:
    print("nvmlInit_v2 feilet:", ERR.get(rc, rc))
    sys.exit(1)

dev = C.c_void_p()
t0 = time.perf_counter_ns()
lib.nvmlDeviceGetHandleByIndex_v2(0, C.byref(dev))
t_handle = (time.perf_counter_ns() - t0) / 1e6
d = dev

# ---- system ----
b = s(); call("System", "Driverversjon", "nvmlSystemGetDriverVersion", (b, 96), lambda: b.value.decode())
b2 = s(); call("System", "NVML-versjon", "nvmlSystemGetNVMLVersion", (b2, 96), lambda: b2.value.decode())
cv = C.c_int(); call("System", "CUDA-driverversjon", "nvmlSystemGetCudaDriverVersion_v2", (C.byref(cv),), lambda: f"{cv.value // 1000}.{cv.value % 1000 // 10}")
cnt = u(); call("System", "Antall GPU-er", "nvmlDeviceGetCount_v2", (C.byref(cnt),), lambda: cnt.value)

# ---- identitet ----
G = "Identitet"
b = s(); call(G, "Navn", "nvmlDeviceGetName", (d, b, 96), lambda: b.value.decode())
x = C.c_int(); call(G, "Merke (brand-enum)", "nvmlDeviceGetBrand", (d, C.byref(x)), lambda: x.value)
a = u(); call(G, "Arkitektur (enum)", "nvmlDeviceGetArchitecture", (d, C.byref(a)), lambda: a.value)
bu = s(); call(G, "UUID", "nvmlDeviceGetUUID", (d, bu, 96), lambda: bu.value.decode())
bs = s(); call(G, "Serienummer", "nvmlDeviceGetSerial", (d, bs, 96), lambda: bs.value.decode())
bp = s(); call(G, "Board part number", "nvmlDeviceGetBoardPartNumber", (d, bp, 96), lambda: bp.value.decode())
bv = s(); call(G, "VBIOS", "nvmlDeviceGetVbiosVersion", (d, bv, 96), lambda: bv.value.decode())
bi = s(); call(G, "InfoROM image", "nvmlDeviceGetInforomImageVersion", (d, bi, 96), lambda: bi.value.decode())
for obj, nm in ((0, "OEM"), (1, "ECC"), (2, "POWER")):
    bo = s(); call(G, f"InfoROM-objekt {nm}", "nvmlDeviceGetInforomVersion", (d, obj, bo, 96), lambda bo=bo: bo.value.decode())
bid = u(); call(G, "Board ID", "nvmlDeviceGetBoardId", (d, C.byref(bid)), lambda: hex(bid.value))
mg = u(); call(G, "Multi-GPU-kort", "nvmlDeviceGetMultiGpuBoard", (d, C.byref(mg)), lambda: mg.value)
mn = u(); call(G, "Minor number", "nvmlDeviceGetMinorNumber", (d, C.byref(mn)), lambda: mn.value)
ma, mi = C.c_int(), C.c_int(); call(G, "Compute capability", "nvmlDeviceGetCudaComputeCapability", (d, C.byref(ma), C.byref(mi)), lambda: f"{ma.value}.{mi.value}")
gsp = s(64); call(G, "GSP-fastvare", "nvmlDeviceGetGspFirmwareVersion", (d, gsp), lambda: gsp.value.decode())
ge, gd = u(), u(); call(G, "GSP-modus (på, standard)", "nvmlDeviceGetGspFirmwareMode", (d, C.byref(ge), C.byref(gd)), lambda: (ge.value, gd.value))
cores = u(); call(G, "Antall CUDA-kjerner", "nvmlDeviceGetNumGpuCores", (d, C.byref(cores)), lambda: cores.value)
bw = u(); call(G, "Minnebuss (bit)", "nvmlDeviceGetMemoryBusWidth", (d, C.byref(bw)), lambda: bw.value)
bt = C.c_int(); call(G, "Busstype (enum)", "nvmlDeviceGetBusType", (d, C.byref(bt)), lambda: bt.value)
irq = u(); call(G, "IRQ", "nvmlDeviceGetIrqNum", (d, C.byref(irq)), lambda: irq.value)
x2 = C.c_int(); call(G, "Display mode", "nvmlDeviceGetDisplayMode", (d, C.byref(x2)), lambda: x2.value)
x3 = C.c_int(); call(G, "Display active", "nvmlDeviceGetDisplayActive", (d, C.byref(x3)), lambda: x3.value)
x4 = C.c_int(); call(G, "Persistence mode", "nvmlDeviceGetPersistenceMode", (d, C.byref(x4)), lambda: x4.value)
x5 = C.c_int(); call(G, "Compute mode", "nvmlDeviceGetComputeMode", (d, C.byref(x5)), lambda: x5.value)
x6 = C.c_int(); call(G, "Accounting mode", "nvmlDeviceGetAccountingMode", (d, C.byref(x6)), lambda: x6.value)
v1, v2 = C.c_int(), C.c_int(); call(G, "ECC-modus", "nvmlDeviceGetEccMode", (d, C.byref(v1), C.byref(v2)), lambda: (v1.value, v2.value))
nvs = C.c_int(); call(G, "NVLink 0", "nvmlDeviceGetNvLinkState", (d, 0, C.byref(nvs)), lambda: nvs.value)
vm = C.c_int(); call(G, "Virtualiseringsmodus", "nvmlDeviceGetVirtualizationMode", (d, C.byref(vm)), lambda: vm.value)
ab1, ab2 = C.c_int(), C.c_int(); call(G, "Auto boost", "nvmlDeviceGetAutoBoostedClocksEnabled", (d, C.byref(ab1), C.byref(ab2)), lambda: (ab1.value, ab2.value))
ps = C.c_int(); call(G, "Strømkilde", "nvmlDeviceGetPowerSource", (d, C.byref(ps)), lambda: ps.value)

# ---- PCI/PCIe ----
G = "PCIe"
pci = PciInfo(); call(G, "PCI-info", "nvmlDeviceGetPciInfo_v3", (d, C.byref(pci)),
                      lambda: dict(busId=pci.busId.decode(), dev=hex(pci.pciDeviceId), sub=hex(pci.pciSubSystemId)))
for name, lab in (("nvmlDeviceGetCurrPcieLinkGeneration", "Gen nå"), ("nvmlDeviceGetMaxPcieLinkGeneration", "Gen maks (system)"),
                  ("nvmlDeviceGetGpuMaxPcieLinkGeneration", "Gen maks (GPU)"), ("nvmlDeviceGetCurrPcieLinkWidth", "Bredde nå"),
                  ("nvmlDeviceGetMaxPcieLinkWidth", "Bredde maks"), ("nvmlDeviceGetPcieSpeed", "Hastighet nå (MB/s per lane?)"),
                  ("nvmlDeviceGetPcieLinkMaxSpeed", "Maks hastighet"), ("nvmlDeviceGetPcieReplayCounter", "Replay-teller")):
    v = u(); call(G, lab, name, (d, C.byref(v)), lambda v=v: v.value)
for ctr, lab in ((0, "TX-gjennomstrømning"), (1, "RX-gjennomstrømning")):
    v = u(); call(G, lab, "nvmlDeviceGetPcieThroughput", (d, ctr, C.byref(v)), lambda v=v: v.value, reps=5, unit="KB/s")

# ---- temperatur ----
G = "Temperatur"
t = u(); call(G, "GPU-temperatur", "nvmlDeviceGetTemperature", (d, 0, C.byref(t)), lambda: t.value, unit="°C")
tv = TempV(version=ver(TempV, 1), sensorType=0); call(G, "GPU-temperatur (V)", "nvmlDeviceGetTemperatureV", (d, C.byref(tv)), lambda: tv.temperature, unit="°C")
for typ, lab in ((0, "Avstenging"), (1, "Nedklokking"), (2, "Minne maks"), (3, "GPU maks"), (4, "Akustisk min"),
                 (5, "Akustisk mål"), (6, "Akustisk maks"), (7, "GPS nå")):
    v = u(); call(G, f"Terskel: {lab}", "nvmlDeviceGetTemperatureThreshold", (d, typ, C.byref(v)), lambda v=v: v.value, unit="°C")
mt = Margin(version=ver(Margin, 1)); call(G, "Margin til grense (T.Limit)", "nvmlDeviceGetMarginTemperature", (d, C.byref(mt)), lambda: mt.marginTemperature, unit="°C")
th = ThermalSettings(); call(G, "Termiske sensorer (alle)", "nvmlDeviceGetThermalSettings", (d, 15, C.byref(th)),
                             lambda: [dict(ctrl=th.sensor[i].controller, min=th.sensor[i].defaultMinTemp, max=th.sensor[i].defaultMaxTemp,
                                           cur=th.sensor[i].currentTemp, target=th.sensor[i].target) for i in range(min(th.count, 3))])

# ---- effekt ----
G = "Effekt"
p = u(); call(G, "Effekttrekk", "nvmlDeviceGetPowerUsage", (d, C.byref(p)), lambda: p.value, unit="mW")
e = C.c_ulonglong(); call(G, "Energi totalt", "nvmlDeviceGetTotalEnergyConsumption", (d, C.byref(e)), lambda: e.value, unit="mJ")
pl = u(); call(G, "Effektgrense", "nvmlDeviceGetPowerManagementLimit", (d, C.byref(pl)), lambda: pl.value, unit="mW")
pmin, pmax = u(), u(); call(G, "Grenseområde", "nvmlDeviceGetPowerManagementLimitConstraints", (d, C.byref(pmin), C.byref(pmax)), lambda: (pmin.value, pmax.value), unit="mW")
pd = u(); call(G, "Standardgrense", "nvmlDeviceGetPowerManagementDefaultLimit", (d, C.byref(pd)), lambda: pd.value, unit="mW")
pe = u(); call(G, "Håndhevet grense", "nvmlDeviceGetEnforcedPowerLimit", (d, C.byref(pe)), lambda: pe.value, unit="mW")
pst = C.c_int(); call(G, "P-tilstand", "nvmlDeviceGetPerformanceState", (d, C.byref(pst)), lambda: f"P{pst.value}")
pst2 = C.c_int(); call(G, "P-tilstand (PowerState, utfaset)", "nvmlDeviceGetPowerState", (d, C.byref(pst2)), lambda: f"P{pst2.value}")
arr = (C.c_int * 16)(); call(G, "Støttede P-tilstander", "nvmlDeviceGetSupportedPerformanceStates", (d, arr, 16), lambda: sorted({f"P{v}" for v in arr if v != 32}, key=lambda x: int(x[1:])))
for pol, lab in ((0, "effekt"), (1, "termisk"), (2, "sync boost"), (3, "kortgrense"), (4, "lav last"), (5, "pålitelighet")):
    vi = Violation(); call(G, f"Brudd-tid: {lab}", "nvmlDeviceGetViolationStatus", (d, pol, C.byref(vi)), lambda vi=vi: (vi.referenceTime, vi.violationTime), unit="ns")

# ---- klokker ----
G = "Klokker"
CL = ((0, "grafikk"), (1, "SM"), (2, "minne"), (3, "video"))
for typ, lab in CL:
    v = u(); call(G, f"Nå: {lab}", "nvmlDeviceGetClockInfo", (d, typ, C.byref(v)), lambda v=v: v.value, unit="MHz")
for typ, lab in CL:
    v = u(); call(G, f"Maks: {lab}", "nvmlDeviceGetMaxClockInfo", (d, typ, C.byref(v)), lambda v=v: v.value, unit="MHz")
for typ, lab in CL[:3]:
    v = u(); call(G, f"Applications: {lab}", "nvmlDeviceGetApplicationsClock", (d, typ, C.byref(v)), lambda v=v: v.value, unit="MHz")
    v = u(); call(G, f"Standard applications: {lab}", "nvmlDeviceGetDefaultApplicationsClock", (d, typ, C.byref(v)), lambda v=v: v.value, unit="MHz")
    v = u(); call(G, f"Customer boost maks: {lab}", "nvmlDeviceGetMaxCustomerBoostClock", (d, typ, C.byref(v)), lambda v=v: v.value, unit="MHz")
for pstate in (0, 2, 5, 8):
    for typ, lab in ((0, "grafikk"), (2, "minne")):
        lo, hi = u(), u()
        call(G, f"P{pstate} {lab} min–maks", "nvmlDeviceGetMinMaxClockOfPState", (d, typ, pstate, C.byref(lo), C.byref(hi)), lambda lo=lo, hi=hi: (lo.value, hi.value), unit="MHz")
for pstate in (0, 8):
    for typ, lab in ((0, "grafikk"), (2, "minne")):
        co = ClockOffset(version=ver(ClockOffset, 1), type=typ, pstate=pstate)
        call(G, f"Offset P{pstate} {lab} (nå, min, maks)", "nvmlDeviceGetClockOffsets", (d, C.byref(co)),
             lambda co=co: (co.clockOffsetMHz, co.minClockOffsetMHz, co.maxClockOffsetMHz), unit="MHz")
n = u(); mem_clocks = (C.c_uint * 32)(); n.value = 32
call(G, "Støttede minneklokker", "nvmlDeviceGetSupportedMemoryClocks", (d, C.byref(n), mem_clocks), lambda: list(mem_clocks[: n.value]), reps=1, unit="MHz")
pm = StrV(version=ver(StrV, 1)); call(G, "Ytelsesmodi (tekst)", "nvmlDeviceGetPerformanceModes", (d, C.byref(pm)), lambda: pm.str.decode()[:400])
cf = StrV(version=ver(StrV, 1)); call(G, "Gjeldende klokker (tekst)", "nvmlDeviceGetCurrentClockFreqs", (d, C.byref(cf)), lambda: cf.str.decode()[:400])
r = C.c_ulonglong(); call(G, "Klokkehendelser nå (maske)", "nvmlDeviceGetCurrentClocksEventReasons", (d, C.byref(r)), lambda: hex(r.value))
r2 = C.c_ulonglong(); call(G, "Støttede klokkehendelser (maske)", "nvmlDeviceGetSupportedClocksEventReasons", (d, C.byref(r2)), lambda: hex(r2.value))

# ---- last ----
G = "Last"
ut = Util(); call(G, "GPU / minnekontroller", "nvmlDeviceGetUtilizationRates", (d, C.byref(ut)), lambda: (ut.gpu, ut.memory), unit="%")
for name, lab in (("nvmlDeviceGetEncoderUtilization", "Encoder"), ("nvmlDeviceGetDecoderUtilization", "Decoder"),
                  ("nvmlDeviceGetJpgUtilization", "JPEG"), ("nvmlDeviceGetOfaUtilization", "OFA")):
    v, sp = u(), u(); call(G, lab, name, (d, C.byref(v), C.byref(sp)), lambda v=v, sp=sp: (v.value, f"periode {sp.value} µs"), unit="%")
sc, fps, lat = u(), u(), u(); call(G, "Encoder-økter", "nvmlDeviceGetEncoderStats", (d, C.byref(sc), C.byref(fps), C.byref(lat)), lambda: (sc.value, fps.value, lat.value))
dp = DynPstates(); call(G, "Dynamiske P-tilstander (GPU, FB, VID, BUS)", "nvmlDeviceGetDynamicPstatesInfo", (d, C.byref(dp)),
                        lambda: [(i, dp.utilization[i].percentage) for i in range(4) if dp.utilization[i].bIsPresent])

# ---- vifter ----
G = "Vifte"
nf = u(); call(G, "Antall vifter", "nvmlDeviceGetNumFans", (d, C.byref(nf)), lambda: nf.value)
fs = u(); call(G, "Viftefart (samlet)", "nvmlDeviceGetFanSpeed", (d, C.byref(fs)), lambda: fs.value, unit="%")
fmin, fmax = u(), u(); call(G, "Min–maks fart", "nvmlDeviceGetMinMaxFanSpeed", (d, C.byref(fmin), C.byref(fmax)), lambda: (fmin.value, fmax.value), unit="%")
for i in range(max(nf.value, 1)):
    v = u(); call(G, f"Vifte {i} fart", "nvmlDeviceGetFanSpeed_v2", (d, i, C.byref(v)), lambda v=v: v.value, unit="%")
    v = u(); call(G, f"Vifte {i} mål", "nvmlDeviceGetTargetFanSpeed", (d, i, C.byref(v)), lambda v=v: v.value, unit="%")
    v = u(); call(G, f"Vifte {i} policy", "nvmlDeviceGetFanControlPolicy_v2", (d, i, C.byref(v)), lambda v=v: v.value)
    fr = FanRpm(version=ver(FanRpm, 1), fan=i); call(G, f"Vifte {i} turtall", "nvmlDeviceGetFanSpeedRPM", (d, C.byref(fr)), lambda fr=fr: fr.speed, unit="o/min")
    co = Cooler(version=ver(Cooler, 1), index=i); call(G, f"Kjøler {i} info", "nvmlDeviceGetCoolerInfo", (d, C.byref(co)), lambda co=co: (co.signalType, co.target))

# ---- minne ----
G = "Minne"
m = Mem(); call(G, "Videominne (v1)", "nvmlDeviceGetMemoryInfo", (d, C.byref(m)), lambda: tuple(v // 2**20 for v in (m.total, m.used, m.free)), unit="MiB tot/brukt/ledig")
m2 = MemV2(version=ver(MemV2, 2)); call(G, "Videominne (v2)", "nvmlDeviceGetMemoryInfo_v2", (d, C.byref(m2)), lambda: tuple(v // 2**20 for v in (m2.total, m2.reserved, m2.used, m2.free)), unit="MiB tot/res/brukt/ledig")
b1 = Bar1(); call(G, "BAR1", "nvmlDeviceGetBAR1MemoryInfo", (d, C.byref(b1)), lambda: tuple(v // 2**20 for v in (b1.bar1Total, b1.bar1Used)), unit="MiB tot/brukt")

# ---- prosesser ----
G = "Prosesser"
for name, lab in (("nvmlDeviceGetComputeRunningProcesses_v3", "Compute"), ("nvmlDeviceGetGraphicsRunningProcesses_v3", "Grafikk")):
    pc = u(); pc.value = 64; pa = (ProcInfo * 64)()
    call(G, lab, name, (d, C.byref(pc), pa), lambda pc=pc, pa=pa: [(pa[i].pid, pa[i].usedGpuMemory // 2**20 if pa[i].usedGpuMemory < 2**62 else None) for i in range(pc.value)], reps=5)
pu = u(); pu.value = 1024; pua = (ProcUtil * 1024)()
call(G, "Last per prosess", "nvmlDeviceGetProcessUtilization", (d, pua, C.byref(pu), C.c_ulonglong(0)),
     lambda: [(pua[i].pid, pua[i].smUtil, pua[i].memUtil) for i in range(pu.value)][:8], reps=1)

# ---- feltverdier (samlet lesing) ----
G = "Feltverdier"
FIELDS = {82: "minnetemperatur", 185: "effekt snitt", 186: "effekt øyeblikk", 187: "grense min", 188: "grense maks",
          189: "grense standard", 190: "grense nå", 191: "energi (mJ)", 192: "grense ønsket"}
fv = (FieldValue * len(FIELDS))()
for i, fid in enumerate(FIELDS):
    fv[i].fieldId = fid
call(G, "Felt " + ",".join(map(str, FIELDS)), "nvmlDeviceGetFieldValues", (d, len(FIELDS), fv),
     lambda: {f"{fv[i].fieldId} {FIELDS[fv[i].fieldId]}": (fv[i].value if fv[i].nvmlReturn == 0 else ERR.get(fv[i].nvmlReturn)) for i in range(len(FIELDS))})

# ---- effekt: GetPowerUsage mot felt 186 (øyeblikk), rett etter hverandre ----
pw_a = u(); fva = (FieldValue * 1)(); fva[0].fieldId = 186
pairs = []
for _ in range(10):
    fn("nvmlDeviceGetPowerUsage")(d, C.byref(pw_a)); fn("nvmlDeviceGetFieldValues")(d, 1, fva)
    pairs.append((pw_a.value, fva[0].value if fva[0].nvmlReturn == 0 else None))
    time.sleep(0.1)
results.append(dict(group="Effekt", label="PowerUsage mot felt 186 (10 par, 100 ms)", fn="sammenligning", rc="OK", value=pairs, us=None, unit="mW"))

# ---- samlet pollerunde (det appen ville lest hvert sekund) ----
rnd = []
tt, pp, ee, cg, cm, cs, cv2, f0, f1, tg0, pg, pw, rr, pls, b1x = (u(), u(), C.c_ulonglong(), u(), u(), u(), u(), u(), u(), u(), u(), u(), C.c_ulonglong(), u(), Bar1())
ut2, m3, pst3 = Util(), Mem(), C.c_int()
enc, encp, dec, decp = u(), u(), u(), u()
F = {k: fn(k) for k in ("nvmlDeviceGetTemperature", "nvmlDeviceGetPowerUsage", "nvmlDeviceGetTotalEnergyConsumption", "nvmlDeviceGetClockInfo",
                         "nvmlDeviceGetUtilizationRates", "nvmlDeviceGetMemoryInfo", "nvmlDeviceGetPerformanceState", "nvmlDeviceGetFanSpeed_v2",
                         "nvmlDeviceGetTargetFanSpeed", "nvmlDeviceGetCurrPcieLinkGeneration", "nvmlDeviceGetCurrPcieLinkWidth",
                         "nvmlDeviceGetCurrentClocksEventReasons", "nvmlDeviceGetEnforcedPowerLimit", "nvmlDeviceGetEncoderUtilization",
                         "nvmlDeviceGetDecoderUtilization")}


def poll_round():
    F["nvmlDeviceGetTemperature"](d, 0, C.byref(tt))
    F["nvmlDeviceGetPowerUsage"](d, C.byref(pp))
    F["nvmlDeviceGetTotalEnergyConsumption"](d, C.byref(ee))
    for typ, out in ((0, cg), (1, cs), (2, cm), (3, cv2)):
        F["nvmlDeviceGetClockInfo"](d, typ, C.byref(out))
    F["nvmlDeviceGetUtilizationRates"](d, C.byref(ut2))
    F["nvmlDeviceGetEncoderUtilization"](d, C.byref(enc), C.byref(encp))
    F["nvmlDeviceGetDecoderUtilization"](d, C.byref(dec), C.byref(decp))
    F["nvmlDeviceGetMemoryInfo"](d, C.byref(m3))
    F["nvmlDeviceGetPerformanceState"](d, C.byref(pst3))
    F["nvmlDeviceGetFanSpeed_v2"](d, 0, C.byref(f0))
    F["nvmlDeviceGetFanSpeed_v2"](d, 1, C.byref(f1))
    F["nvmlDeviceGetTargetFanSpeed"](d, 0, C.byref(tg0))
    F["nvmlDeviceGetCurrPcieLinkGeneration"](d, C.byref(pg))
    F["nvmlDeviceGetCurrPcieLinkWidth"](d, C.byref(pw))
    F["nvmlDeviceGetCurrentClocksEventReasons"](d, C.byref(rr))
    F["nvmlDeviceGetEnforcedPowerLimit"](d, C.byref(pls))


for _ in range(5 if SLOW else 50):
    if SLOW:
        time.sleep(1)
    a = time.perf_counter_ns(); poll_round(); rnd.append((time.perf_counter_ns() - a) / 1000)

lib.nvmlShutdown()

summary = dict(dlopen_ms=round(t_dlopen, 2), init_ms=round(t_init, 2), handle_ms=round(t_handle, 3),
               poll_round_us_median=round(statistics.median(rnd), 1), poll_round_us_min=round(min(rnd), 1),
               poll_round_us_max=round(max(rnd), 1), poll_calls=19)

if "--json" in sys.argv:
    print(json.dumps(dict(summary=summary, results=results), ensure_ascii=False, indent=1, default=str))
else:
    print(json.dumps(summary, ensure_ascii=False))
    for r in results:
        val = r["value"] if r["rc"] == "OK" else ""
        print(f'{r["group"]:<11} {r["label"]:<42} {r["rc"]:<20} {r["us"] if r["us"] is not None else "-":>9} µs  {val} {r["unit"] if r["rc"] == "OK" else ""}')
