// Menneskelige navn på hwmon-enheter, sensorer og nettverkskort. Det rå
// navnet (f.eks. ucsi_source_psy_8_00081 · curr1) står i verktøytipset.

const DEVICES = [
  [/^k10temp$/, 'Prosessor'],
  [/^coretemp$/, 'Prosessor'],
  [/^zenpower$/, 'Prosessor'],
  [/^iwlwifi/, 'Wi-Fi-kort'],
  [/^ath\d+k/, 'Wi-Fi-kort'],
  [/^mt7\d+/, 'Wi-Fi-kort'],
  [/^ucsi_source_psy/, 'USB-C-kontroller (skjermkort)'],
  [/^hidpp_battery/, 'Batteri i trådløs mus/tastatur'],
  [/^nvme$/, 'NVMe-disk'],
  [/^drivetemp$/, 'Disk'],
  [/^amdgpu$/, 'AMD-grafikk'],
  [/^nouveau$/, 'Skjermkort (nouveau)'],
  [/^acpitz$/, 'ACPI-termisk sone'],
  [/^(nct\d+|it\d+|f71\d+|w83\d+)/, 'Hovedkort'],
  [/^(asus|gigabyte_wmi|dell_smm|thinkpad)/, 'Hovedkort'],
  [/^(spd5118|jc42)$/, 'Minnemodul'],
  [/^(BAT\d|battery)/i, 'Batteri'],
  [/^(r8169|igb|e1000)/, 'Nettverkskort'],
  [/^cros_ec/, 'Innebygd kontroller'],
];

const KIND = { temp: 'Temperatur', fan: 'Vifte', in: 'Spenning', power: 'Effekt', curr: 'Strøm' };

export function deviceName(raw) {
  for (const [re, name] of DEVICES) if (re.test(raw)) return name;
  return raw;
}

/** Navn på én sensor: «USB-C-kontroller (skjermkort) · strøm». */
export function sensorName(dev, x) {
  const d = deviceName(dev.name);
  // Etiketter som bare gjentar nøkkelen (curr1, temp1) erstattes med typen.
  const generic = !x.label || x.label === x.key || /^(temp|fan|in|power|curr)\d+$/.test(x.label);
  const label = generic ? KIND[x.kind] || x.kind : x.label;
  return { name: `${d} · ${generic ? label.toLowerCase() : label}`, raw: `${dev.name} · ${x.key} (${x.id.split('/')[0]})` };
}

const VIRTUAL = /^(docker|veth|br-|virbr|vnet|tap|tun|wg|lxc|lxd|vmnet|zt)/;

/** «Wi-Fi (wlo1)», «Ethernet (enp24s0)». */
export function netName(name, n) {
  if (VIRTUAL.test(name)) return `Virtuelt (${name})`;
  if (n?.wireless || /^wl/.test(name)) return `Wi-Fi (${name})`;
  if (/^(en|eth)/.test(name)) return `Ethernet (${name})`;
  if (/^ww/.test(name)) return `Mobilnett (${name})`;
  return name;
}

export const isPhysicalNet = (name) => !VIRTUAL.test(name);

/** CPU-temperatur: Tctl/Tdie (AMD), Package (Intel) eller første temp. */
export function cpuTempSensor(sys) {
  const dev = sys?.hwmon?.find((d) => d.name === 'k10temp' || d.name === 'coretemp' || d.name === 'zenpower');
  if (!dev) return null;
  return dev.sensors.find((x) => x.label === 'Tctl') || dev.sensors.find((x) => x.label === 'Tdie') || dev.sensors.find((x) => /Package/.test(x.label)) || dev.sensors.find((x) => x.kind === 'temp') || null;
}
export const cpuTemp = (sys) => cpuTempSensor(sys)?.value ?? null;

/** Sum ned/opp over fysiske nettverkskort som er oppe (byte/s). */
export function netTotals(sys) {
  let rx = null;
  let tx = null;
  for (const [name, n] of Object.entries(sys?.net || {})) {
    if (!isPhysicalNet(name)) continue;
    if (n.rx != null) rx = (rx ?? 0) + n.rx;
    if (n.tx != null) tx = (tx ?? 0) + n.tx;
  }
  return { rx, tx };
}

export const memUsedPct = (sys) => (sys?.mem?.usedKB != null && sys.mem.totalKB ? (sys.mem.usedKB / sys.mem.totalKB) * 100 : null);
