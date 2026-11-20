//! Én klient: hva den har sagt hei med, hva den abonnerer på, takten den vil
//! ha, og hvilke sider som synes i vinduet (`set_focus`).

use crate::tasks::TaskSnapshot;
use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct Session {
    pub id: u64,
    pub hello: bool,
    pub client: String,
    pub pid: Option<i32>,
    pub topics: BTreeSet<String>,
    /// Takten klienten ba om i `subscribe` (500–2000 ms).
    pub interval_ms: Option<u32>,
    /// Sidene som synes. `Some([])` betyr at vinduet er minimert eller skjult.
    pub focus: Option<Vec<String>>,
    /// Sist viste bilde og ett svar som kan være underveis til klienten.
    pub task_snapshots: VecDeque<(u64, Rc<TaskSnapshot>)>,
}

impl Session {
    pub fn new(id: u64, pid: Option<i32>) -> Session {
        Session { id, pid, ..Default::default() }
    }

    pub fn wants(&self, topic: &str) -> bool {
        self.hello && self.topics.contains(topic)
    }

    /// Vinduet er minimert: sakte prøver.
    pub fn hidden(&self) -> bool {
        matches!(&self.focus, Some(f) if f.is_empty())
    }
}
