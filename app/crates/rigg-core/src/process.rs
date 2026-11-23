//! Prosessmodell og avgrensede, rene søk for Oppgavebehandling.
//!
//! CPU: 100 % er én logisk CPU. Manglende rater er ukjent, aldri null.
//!
//! Minne. Kolonnen «Minne» er prosessens private minne i RAM: `RssAnon` fra
//! `/proc/PID/status` (anonyme sider som heap og stakk), det samme som
//! «resident − delt» i `statm`, slik GNOME Ressurser og Systemovervåker regner.
//! Det kan leses for alle prosesser, også andre brukeres, og summen av alle
//! radene ligger nær `AnonPages` i `/proc/meminfo`. RSS tar også med delte
//! sider (biblioteker, filer og delt minne) og kan derfor ikke summeres.
//! Delt minne (`RssFile` + `RssShmem`) og `VmSwap` vises for seg. PSS og USS
//! fra `smaps_rollup` koster mye kjernetid og kan bare leses for egne
//! prosesser, så de hentes bare for den valgte prosessen ([`ProcessDetail`]).
use crate::{tr, trf};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

pub const MAX_PAGE_ROWS: usize = 100;
/// Gir rom til protokollens konvolutt innenfor grensen på 64 KiB.
pub const MAX_PAGE_BYTES: usize = 48 * 1024;
/// Høyst så mange utvidede grupper per forespørsel, og så lange nøkler.
pub const MAX_EXPANDED: usize = 32;
pub const MAX_GROUP_KEY: usize = 80;
/// Gruppenøkkelen for alle kjernetråder.
pub const KERNEL_GROUP: &str = "kernel";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_ticks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    #[serde(flatten)]
    pub identity: ProcessIdentity,
    pub ppid: u32,
    pub name: String,
    pub state: String,
    pub uid: Option<u32>,
    pub user: Option<String>,
    pub effective_uid: Option<u32>,
    /// Resident minne (RSS), inkludert delte sider.
    pub rss_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub gpu_vram_bytes: Option<u64>,
    pub can_terminate: bool,
    pub protected_reason: Option<String>,
    #[serde(default)]
    pub depth: u16,
    /// Privat minne i RAM (`RssAnon`). Kolonnen «Minne».
    #[serde(default)]
    pub private_bytes: Option<u64>,
    /// Delte sider i RAM (`RssFile` + `RssShmem`).
    #[serde(default)]
    pub shared_bytes: Option<u64>,
    /// Minne flyttet til swap (`VmSwap`).
    #[serde(default)]
    pub swap_bytes: Option<u64>,
    #[serde(default)]
    pub threads: Option<u32>,
    /// Kjernetråd (ingen egen brukerprosess, ingen minnetall).
    #[serde(default)]
    pub kernel: bool,
    /// Gruppevisning: antall prosesser i gruppen (0 for en enkelt prosess).
    #[serde(default)]
    pub group_count: u32,
    /// Gruppevisning: nøkkelen til gruppen raden er eller hører til.
    #[serde(default)]
    pub group_key: Option<String>,
    /// Gruppevisning: gruppen er foldet ut.
    #[serde(default)]
    pub expanded: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    pub sampled_at_ms: i64,
    pub rows: Vec<ProcessRow>,
    pub unreadable: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessSort {
    Pid,
    Name,
    #[default]
    Cpu,
    /// Privat minne (kolonnen «Minne»).
    Memory,
    Swap,
    GpuMemory,
    User,
    State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProcessQuery {
    pub filter: String,
    pub sort: ProcessSort,
    pub descending: bool,
    pub tree: bool,
    /// Grupper prosessene etter navn (alle `chrome` samlet), og alle
    /// kjernetråder i én gruppe. Går foran `tree`.
    pub group: bool,
    /// Gruppene som er foldet ut (nøklene fra `groupKey`).
    pub expanded: Vec<String>,
    pub offset: usize,
    pub limit: usize,
}

impl Default for ProcessQuery {
    fn default() -> Self {
        Self {
            filter: String::new(),
            sort: ProcessSort::Cpu,
            descending: true,
            tree: false,
            group: false,
            expanded: Vec::new(),
            offset: 0,
            limit: MAX_PAGE_ROWS,
        }
    }
}

/// Summer over hele prosessbildet (uavhengig av søk og side).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProcessTotals {
    /// Brukerprosesser (uten kjernetråder).
    pub processes: u32,
    pub kernel_threads: u32,
    /// Tråder i brukerprosessene som kunne leses, pluss kjernetrådene.
    pub threads: u64,
    /// Summen av privat minne (`RssAnon`) for prosessene som kunne leses.
    pub private_bytes: u64,
    pub swap_bytes: u64,
    /// Brukerprosesser uten lesbare minnetall.
    pub memory_unknown: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPage {
    pub sampled_at_ms: i64,
    pub rows: Vec<ProcessRow>,
    pub total: usize,
    pub matched: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub unreadable: u32,
    pub error: Option<String>,
    #[serde(default)]
    pub totals: ProcessTotals,
}

/// Felt i [`ProcessDetail`] som ikke kunne leses fordi prosessen tilhører en
/// annen bruker eller root (kjernen krever samme bruker eller root).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Restricted {
    Exe,
    Smaps,
    Io,
}

/// Mer om én prosess, lest på forespørsel for den valgte raden.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProcessDetail {
    #[serde(flatten)]
    pub identity: ProcessIdentity,
    pub sampled_at_ms: i64,
    pub name: String,
    pub cmdline: Option<String>,
    pub exe: Option<String>,
    pub ppid: u32,
    pub parent_name: Option<String>,
    pub uid: Option<u32>,
    pub user: Option<String>,
    pub state: String,
    pub threads: Option<u32>,
    pub nice: Option<i32>,
    pub started_at_ms: Option<i64>,
    pub cpu_time_s: Option<f64>,
    pub rss_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    pub shared_bytes: Option<u64>,
    pub swap_bytes: Option<u64>,
    /// Proporsjonal andel (delte sider delt på antall brukere).
    pub pss_bytes: Option<u64>,
    /// Bare denne prosessens sider (Private_Clean + Private_Dirty).
    pub uss_bytes: Option<u64>,
    pub io_read_bytes: Option<u64>,
    pub io_write_bytes: Option<u64>,
    /// Systemd-enheten fra cgroup (for eksempel `app-gnome-firefox-123.scope`).
    pub unit: Option<String>,
    pub kernel: bool,
    pub restricted: Vec<Restricted>,
}

fn optional_order<T: PartialOrd>(a: Option<T>, b: Option<T>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let order = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if descending { order.reverse() } else { order }
        }
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn compare(a: &ProcessRow, b: &ProcessRow, q: &ProcessQuery) -> Ordering {
    let mut order = match q.sort {
        ProcessSort::Cpu => optional_order(a.cpu_percent, b.cpu_percent, q.descending),
        ProcessSort::Memory => optional_order(a.private_bytes, b.private_bytes, q.descending),
        ProcessSort::Swap => optional_order(a.swap_bytes, b.swap_bytes, q.descending),
        ProcessSort::GpuMemory => optional_order(a.gpu_vram_bytes, b.gpu_vram_bytes, q.descending),
        ProcessSort::User => optional_order(a.user.as_deref(), b.user.as_deref(), q.descending)
            .then_with(|| optional_order(a.uid, b.uid, q.descending)),
        ProcessSort::Pid => a.identity.pid.cmp(&b.identity.pid),
        ProcessSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        ProcessSort::State => a.state.cmp(&b.state),
    };
    if q.descending
        && matches!(
            q.sort,
            ProcessSort::Pid | ProcessSort::Name | ProcessSort::State
        )
    {
        order = order.reverse();
    }
    order
        .then_with(|| a.identity.pid.cmp(&b.identity.pid))
        .then_with(|| a.identity.start_ticks.cmp(&b.identity.start_ticks))
}

/// Gruppen en prosess hører til: alle kjernetråder samlet, ellers navnet.
pub fn group_key(row: &ProcessRow) -> String {
    if row.kernel {
        KERNEL_GROUP.into()
    } else {
        format!("name:{}", row.name.chars().take(MAX_GROUP_KEY - 5).collect::<String>())
    }
}

/// Summen av de kjente verdiene, `None` når ingen er kjent.
fn sum<T: Copy + std::iter::Sum<T>>(values: impl Iterator<Item = Option<T>> + Clone) -> Option<T> {
    values.clone().any(|v| v.is_some()).then(|| values.flatten().sum())
}

/// Én rad som står for hele gruppen: summer og felles verdier.
fn group_row(key: &str, members: &[&ProcessRow], expanded: bool) -> ProcessRow {
    let leader = members
        .iter()
        .min_by_key(|r| (r.identity.start_ticks, r.identity.pid))
        .copied()
        .unwrap_or(members[0]);
    let users: HashSet<_> = members.iter().map(|r| r.uid).collect();
    let state = ["R", "D", "S", "I", "T", "t", "Z"]
        .into_iter()
        .find(|s| members.iter().any(|r| r.state == *s))
        .map_or_else(|| leader.state.clone(), str::to_string);
    let n = members.len() as u32;
    ProcessRow {
        identity: leader.identity,
        ppid: 0,
        name: if key == KERNEL_GROUP { tr!("Kjernetråder").into() } else { leader.name.clone() },
        state,
        uid: if users.len() == 1 { leader.uid } else { None },
        user: if users.len() == 1 { leader.user.clone() } else { Some(tr!("flere").into()) },
        effective_uid: None,
        rss_bytes: sum(members.iter().map(|r| r.rss_bytes)),
        cpu_percent: sum(members.iter().map(|r| r.cpu_percent)),
        gpu_vram_bytes: sum(members.iter().map(|r| r.gpu_vram_bytes)),
        can_terminate: false,
        protected_reason: Some(trf!("En gruppe med {antall} prosesser. Fold ut gruppen og velg én prosess for å avslutte den.", antall = n)),
        depth: 0,
        private_bytes: sum(members.iter().map(|r| r.private_bytes)),
        shared_bytes: None,
        swap_bytes: sum(members.iter().map(|r| r.swap_bytes)),
        threads: sum(members.iter().map(|r| r.threads)),
        kernel: key == KERNEL_GROUP,
        group_count: n,
        group_key: Some(key.into()),
        expanded,
    }
}

impl ProcessSnapshot {
    /// Summer over hele bildet. Kjernetråder teller som én tråd hver.
    pub fn totals(&self) -> ProcessTotals {
        let mut t = ProcessTotals::default();
        for row in &self.rows {
            if row.kernel {
                t.kernel_threads += 1;
                t.threads += 1;
                continue;
            }
            t.processes += 1;
            t.threads += u64::from(row.threads.unwrap_or(0));
            match row.private_bytes {
                Some(bytes) => t.private_bytes = t.private_bytes.saturating_add(bytes),
                None => t.memory_unknown += 1,
            }
            t.swap_bytes = t.swap_bytes.saturating_add(row.swap_bytes.unwrap_or(0));
        }
        t
    }

    /// Søk gjelder navn, PID og reell UID. Trevisning viser de filtrerte
    /// prosessene; en forelder som ikke finnes i utvalget blir ikke oppfunnet.
    /// Foreldre som startet etter barnet kobles heller ikke til (PID-gjenbruk).
    /// Gruppevisning samler de filtrerte prosessene etter navn; en gruppe med
    /// én prosess vises som prosessen selv.
    pub fn page(&self, query: &ProcessQuery) -> ProcessPage {
        let needle = query
            .filter
            .trim()
            .chars()
            .take(128)
            .collect::<String>()
            .to_lowercase();
        let mut rows: Vec<_> = self
            .rows
            .iter()
            .filter(|r| {
                needle.is_empty()
                    || r.name.to_lowercase().contains(&needle)
                    || r.identity.pid.to_string().contains(&needle)
                    || r.uid.is_some_and(|u| u.to_string().contains(&needle))
                    || r.user
                        .as_ref()
                        .is_some_and(|u| u.to_lowercase().contains(&needle))
            })
            .collect();
        rows.sort_by(|a, b| compare(a, b, query));
        let mut ordered: Vec<(std::borrow::Cow<ProcessRow>, u16)> = Vec::with_capacity(rows.len());
        if query.group {
            let mut groups: BTreeMap<String, Vec<&ProcessRow>> = BTreeMap::new();
            for row in &rows {
                groups.entry(group_key(row)).or_default().push(*row);
            }
            let mut heads: Vec<(String, ProcessRow, Vec<&ProcessRow>)> = groups
                .into_iter()
                .map(|(key, members)| {
                    let open = query.expanded.contains(&key);
                    let head = if members.len() == 1 {
                        let mut row = members[0].clone();
                        row.group_key = Some(key.clone());
                        row
                    } else {
                        group_row(&key, &members, open)
                    };
                    (key, head, members)
                })
                .collect();
            heads.sort_by(|a, b| compare(&a.1, &b.1, query));
            for (key, head, members) in heads {
                let open = head.group_count > 0 && head.expanded;
                ordered.push((std::borrow::Cow::Owned(head), 0));
                if open {
                    // Medlemmene er allerede sortert (de kom fra `rows`).
                    for member in members {
                        let mut row = member.clone();
                        row.group_key = Some(key.clone());
                        ordered.push((std::borrow::Cow::Owned(row), 1));
                    }
                }
            }
        } else if query.tree {
            let by_pid: HashMap<_, _> = rows.iter().map(|r| (r.identity.pid, *r)).collect();
            let mut children: HashMap<u32, Vec<&ProcessRow>> = HashMap::new();
            let mut roots = Vec::new();
            for row in &rows {
                if row.ppid != row.identity.pid
                    && by_pid
                        .get(&row.ppid)
                        .is_some_and(|p| p.identity.start_ticks <= row.identity.start_ticks)
                {
                    children.entry(row.ppid).or_default().push(*row);
                } else {
                    roots.push(*row);
                }
            }
            let mut seen = HashSet::new();
            // Iterativ traversering tåler dype trær og feilaktige fixtures med sykluser.
            for root in roots.into_iter().chain(rows.iter().copied()) {
                let mut stack = vec![(root, 0u16)];
                while let Some((row, depth)) = stack.pop() {
                    if !seen.insert(row.identity) {
                        continue;
                    }
                    ordered.push((std::borrow::Cow::Borrowed(row), depth));
                    if let Some(kids) = children.get(&row.identity.pid) {
                        stack.extend(
                            kids.iter()
                                .rev()
                                .map(|kid| (*kid, depth.saturating_add(1).min(64))),
                        );
                    }
                }
            }
        } else {
            ordered.extend(rows.into_iter().map(|r| (std::borrow::Cow::Borrowed(r), 0)));
        }
        let matched = ordered.len();
        let limit = query.limit.clamp(1, MAX_PAGE_ROWS);
        let offset = if query.offset < matched {
            query.offset
        } else {
            (matched.saturating_sub(1) / limit) * limit
        };
        let mut page = ProcessPage {
            sampled_at_ms: self.sampled_at_ms,
            rows: vec![],
            total: self.rows.len(),
            matched,
            offset,
            next_offset: None,
            unreadable: self.unreadable,
            error: self.error.as_ref().map(|s| s.chars().take(256).collect()),
            totals: self.totals(),
        };
        for (row, depth) in ordered
            .into_iter()
            .skip(offset)
            .take(limit)
        {
            let mut row = row.into_owned();
            row.name = row.name.chars().take(128).collect();
            row.user = row.user.map(|s| s.chars().take(64).collect());
            row.state = row.state.chars().take(8).collect();
            row.protected_reason = row.protected_reason.map(|s| s.chars().take(160).collect());
            row.group_key = row.group_key.map(|s| s.chars().take(MAX_GROUP_KEY).collect());
            row.depth = depth;
            page.rows.push(row);
            // Reserverer også plass til nextOffset før endelig serialisering.
            page.next_offset = Some(offset + page.rows.len());
            if serde_json::to_vec(&page).map_or(true, |bytes| bytes.len() > MAX_PAGE_BYTES) {
                page.rows.pop();
                break;
            }
        }
        page.next_offset = (offset + page.rows.len() < matched).then_some(offset + page.rows.len());
        page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(pid: u32, ppid: u32, start: u64, cpu: Option<f64>) -> ProcessRow {
        ProcessRow {
            identity: ProcessIdentity {
                pid,
                start_ticks: start,
            },
            ppid,
            name: format!("Arbeid {pid}"),
            state: "S".into(),
            uid: Some(1000),
            user: Some("julian".into()),
            effective_uid: Some(1000),
            rss_bytes: Some(4096),
            cpu_percent: cpu,
            gpu_vram_bytes: None,
            can_terminate: false,
            protected_reason: None,
            ..Default::default()
        }
    }
    #[test]
    fn query_orders_unknown_last_and_pages() {
        let s = ProcessSnapshot {
            rows: vec![
                row(2, 1, 1, None),
                row(3, 1, 2, Some(150.0)),
                row(4, 1, 3, Some(2.0)),
            ],
            ..Default::default()
        };
        let mut q = ProcessQuery {
            limit: 1,
            ..Default::default()
        };
        let p = s.page(&q);
        assert_eq!(p.rows[0].identity.pid, 3);
        assert_eq!(p.next_offset, Some(1));
        q.descending = false;
        assert_eq!(s.page(&q).rows[0].identity.pid, 4);
        q.filter = "arbeid 2".into();
        assert_eq!(s.page(&q).matched, 1);
        q.offset = usize::MAX;
        let page = s.page(&q);
        assert_eq!(page.offset, 0);
        assert_eq!(page.rows[0].identity.pid, 2);
        q.filter.clear();
        q.limit = 2;
        let page = s.page(&q);
        assert_eq!(page.offset, 2);
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.next_offset, None);
    }
    #[test]
    fn tree_handles_orphans_reused_parents_cycles_and_depth() {
        let s = ProcessSnapshot {
            rows: vec![
                row(2, 99, 2, None),
                row(3, 2, 3, None),
                row(4, 5, 4, None),
                row(5, 4, 4, None),
                row(6, 7, 6, None),
                row(7, 1, 7, None),
            ],
            ..Default::default()
        };
        let p = s.page(&ProcessQuery {
            tree: true,
            sort: ProcessSort::Pid,
            descending: false,
            ..Default::default()
        });
        assert_eq!(p.rows.len(), 6);
        assert_eq!(
            p.rows.iter().find(|r| r.identity.pid == 3).unwrap().depth,
            1
        );
        assert_eq!(
            p.rows.iter().find(|r| r.identity.pid == 6).unwrap().depth,
            0
        );
    }
    #[test]
    fn page_is_bounded_even_with_escaped_text() {
        let s = ProcessSnapshot {
            rows: (2..500)
                .map(|pid| {
                    let mut r = row(pid, 1, 1, None);
                    r.name = "\0".repeat(1000);
                    r.protected_reason = Some("\0".repeat(1000));
                    r
                })
                .collect(),
            ..Default::default()
        };
        let p = s.page(&ProcessQuery {
            limit: usize::MAX,
            ..Default::default()
        });
        assert!(!p.rows.is_empty());
        assert!(p.rows.len() <= MAX_PAGE_ROWS);
        assert!(serde_json::to_vec(&p).unwrap().len() <= MAX_PAGE_BYTES);
        assert!(p.next_offset.is_some());
    }

    fn named(pid: u32, name: &str, private: u64, cpu: f64) -> ProcessRow {
        ProcessRow {
            name: name.into(),
            private_bytes: Some(private),
            swap_bytes: Some(pid as u64),
            threads: Some(2),
            ..row(pid, 1, u64::from(pid), Some(cpu))
        }
    }
    #[test]
    fn group_samler_etter_navn_og_folder_ut() {
        let mut kernel = row(2, 0, 1, Some(0.0));
        kernel.name = "kworker/0:1".into();
        kernel.kernel = true;
        kernel.rss_bytes = None;
        let mut kernel2 = kernel.clone();
        kernel2.identity.pid = 3;
        kernel2.name = "ksoftirqd/0".into();
        let s = ProcessSnapshot {
            rows: vec![
                named(10, "chrome", 900, 1.0),
                named(11, "chrome", 800, 2.0),
                named(12, "chrome", 700, 0.5),
                named(20, "gnome-shell", 2000, 3.0),
                named(30, "bash", 5, 0.0),
                kernel,
                kernel2,
            ],
            ..Default::default()
        };
        let mut q = ProcessQuery { group: true, sort: ProcessSort::Memory, ..Default::default() };
        let p = s.page(&q);
        let names: Vec<_> = p.rows.iter().map(|r| (r.name.as_str(), r.group_count)).collect();
        // chrome (2 400) sorteres over gnome-shell (2 000) som gruppe.
        assert_eq!(names, [("chrome", 3), ("gnome-shell", 0), ("bash", 0), ("Kjernetråder", 2)]);
        let chrome = &p.rows[0];
        assert_eq!((chrome.private_bytes, chrome.swap_bytes, chrome.threads), (Some(2400), Some(33), Some(6)));
        assert_eq!(chrome.cpu_percent, Some(3.5));
        assert_eq!(chrome.identity.pid, 10, "lederen er den eldste prosessen");
        assert!(!chrome.can_terminate && chrome.protected_reason.is_some());
        assert_eq!(chrome.group_key.as_deref(), Some("name:chrome"));
        assert_eq!(p.rows[3].group_key.as_deref(), Some(KERNEL_GROUP));
        assert_eq!(p.matched, 4);
        q.expanded = vec!["name:chrome".into()];
        let p = s.page(&q);
        assert!(p.rows[0].expanded);
        let kids: Vec<_> = p.rows[1..4].iter().map(|r| (r.identity.pid, r.depth)).collect();
        assert_eq!(kids, [(10, 1), (11, 1), (12, 1)]);
        assert!(p.rows[1..4].iter().all(|r| r.group_count == 0 && r.group_key.as_deref() == Some("name:chrome")));
        assert_eq!(p.matched, 7);
        // Søket gjelder prosessene; gruppen viser bare treffene.
        q.filter = "11".into();
        let p = s.page(&q);
        assert_eq!(p.rows.len(), 1);
        assert_eq!((p.rows[0].identity.pid, p.rows[0].group_count), (11, 0));
        let t = s.totals();
        assert_eq!((t.processes, t.kernel_threads, t.threads), (5, 2, 12));
        assert_eq!((t.private_bytes, t.swap_bytes, t.memory_unknown), (4405, 83, 0));
    }
    #[test]
    fn group_med_ulike_brukere_og_ukjente_verdier() {
        let mut a = named(10, "python3", 10, 1.0);
        let mut b = named(11, "python3", 20, 1.0);
        b.uid = Some(0);
        b.user = Some("root".into());
        a.cpu_percent = None;
        b.cpu_percent = None;
        a.private_bytes = None;
        let s = ProcessSnapshot { rows: vec![a, b], ..Default::default() };
        let p = s.page(&ProcessQuery { group: true, ..Default::default() });
        let g = &p.rows[0];
        assert_eq!(g.group_count, 2);
        assert_eq!(g.uid, None);
        assert_eq!(g.user.as_deref(), Some("flere"));
        assert_eq!(g.cpu_percent, None, "ingen kjent CPU gir ukjent, ikke null");
        assert_eq!(g.private_bytes, Some(20), "kjente verdier summeres");
        assert_eq!(s.totals().memory_unknown, 1);
    }
}
