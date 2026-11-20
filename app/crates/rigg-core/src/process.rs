//! Prosessmodell og avgrensede, rene søk for Oppgavebehandling.
//!
//! CPU: 100 % er én logisk CPU. RSS inkluderer delte sider og skal ikke
//! summeres til «brukt systemminne». Manglende rater er ukjent, aldri null.
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

pub const MAX_PAGE_ROWS: usize = 100;
/// Gir rom til protokollens konvolutt innenfor grensen på 64 KiB.
pub const MAX_PAGE_BYTES: usize = 48 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_ticks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub rss_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub gpu_vram_bytes: Option<u64>,
    pub can_terminate: bool,
    pub protected_reason: Option<String>,
    #[serde(default)]
    pub depth: u16,
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
    Memory,
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
            offset: 0,
            limit: MAX_PAGE_ROWS,
        }
    }
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
        ProcessSort::Memory => optional_order(a.rss_bytes, b.rss_bytes, q.descending),
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

impl ProcessSnapshot {
    /// Søk gjelder navn, PID og reell UID. Trevisning viser de filtrerte
    /// prosessene; en forelder som ikke finnes i utvalget blir ikke oppfunnet.
    /// Foreldre som startet etter barnet kobles heller ikke til (PID-gjenbruk).
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
        let matched = rows.len();
        let mut ordered = Vec::with_capacity(matched);
        if query.tree {
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
                    ordered.push((row, depth));
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
            ordered.extend(rows.into_iter().map(|r| (r, 0)));
        }
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
        };
        for (row, depth) in ordered
            .into_iter()
            .skip(offset)
            .take(limit)
        {
            let mut row = row.clone();
            row.name = row.name.chars().take(128).collect();
            row.user = row.user.map(|s| s.chars().take(64).collect());
            row.state = row.state.chars().take(8).collect();
            row.protected_reason = row.protected_reason.map(|s| s.chars().take(160).collect());
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
            depth: 0,
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
}
