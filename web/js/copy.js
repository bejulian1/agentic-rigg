// «Kopier som tekst/Markdown»: gjør kv-lister og tabeller om til ren tekst.

const clean = (s) => s.replace(/\s+/g, ' ').trim();

/** Leser par av <dt>/<dd> fra en <dl class="kv">. Knapper tas ikke med. */
export function kvPairs(host) {
  const out = [];
  for (const row of host.querySelectorAll('.kv-row')) {
    const dt = row.querySelector('dt');
    const dd = row.querySelector('dd');
    if (!dt || !dd) continue;
    let text = dd.dataset.copy;
    if (!text) {
      const c = dd.cloneNode(true);
      for (const b of c.querySelectorAll('button')) b.remove();
      text = c.textContent;
    }
    const info = dd.querySelector('.info-btn')?.dataset.tip || null;
    out.push([clean(dt.textContent), clean(text), info]);
  }
  return out;
}

export const kvToText = (host) => kvPairs(host).flatMap(([k, v, i]) => [`${k}: ${v}`, ...(i ? [`  (${i})`] : [])]);
export const kvToMd = (host) => ['| Felt | Verdi |', '|---|---|', ...kvPairs(host).map(([k, v]) => `| ${k} | ${v.replace(/\|/g, '\\|')} |`)];

// Stolpekolonner (td.c-bar) har ingen tekst og tas ikke med.
function rows(table) {
  return [...table.querySelectorAll('tr')].map((tr) => [...tr.children].filter((c) => !c.classList.contains('c-bar')).map((c) => clean(c.textContent)));
}
export const tableToText = (table) => rows(table).map((r) => r.join(' | ').trim());
export function tableToMd(table) {
  const r = rows(table);
  if (!r.length) return [];
  return [`| ${r[0].join(' | ')} |`, `|${r[0].map(() => '---').join('|')}|`, ...r.slice(1).map((x) => `| ${x.join(' | ')} |`)];
}

export async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.append(ta);
      ta.select();
      const ok = document.execCommand('copy');
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}
