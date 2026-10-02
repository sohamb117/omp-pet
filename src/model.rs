//! Small, versioned snapshots. UI state never depends on terminal scraping.
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    #[default]
    Idle,
    Working,
    Waiting,
    Compacting,
    Error,
    Disconnected,
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready",
            Self::Working => "Working",
            Self::Waiting => "Needs your attention",
            Self::Compacting => "Compacting context",
            Self::Error => "Something went wrong",
            Self::Disconnected => "Disconnected",
        }
    }

    pub fn animates(self) -> bool {
        matches!(self, Self::Working | Self::Compacting)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    pub tokens: u64,
    pub window: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub session_id: String,
    pub seq: u64,
    pub project: String,
    pub task: String,
    pub activity: Activity,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub context: Option<ContextUsage>,
}

impl Snapshot {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != PROTOCOL_VERSION {
            return Err("unsupported protocol version");
        }
        if self.session_id.is_empty() || self.session_id.len() > 256 {
            return Err("invalid session id");
        }
        if self.task.len() > 2048
            || self.project.len() > 1024
            || self.tool.as_ref().is_some_and(|s| s.len() > 512)
        {
            return Err("display text exceeds limit");
        }
        if let Some(c) = &self.context {
            if c.window == 0 || !c.percent.is_finite() || !(0.0..=100.0).contains(&c.percent) {
                return Err("invalid context usage");
            }
        }
        Ok(())
    }

    pub fn demo() -> Self {
        Self {
            version: PROTOCOL_VERSION,
            session_id: "demo".into(),
            seq: 1,
            project: "omp-pet".into(),
            task: "Making a little home on your desktop".into(),
            activity: Activity::Working,
            tool: Some("Building the native companion".into()),
            context: Some(ContextUsage {
                tokens: 42000,
                window: 100000,
                percent: 42.0,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_incompatible_and_unbounded_messages() {
        let mut s = Snapshot::demo();
        s.version = 99;
        assert_eq!(s.validate(), Err("unsupported protocol version"));
        s.version = 1;
        s.task = "x".repeat(2049);
        assert!(s.validate().is_err());
    }

    #[test]
    fn accepts_unknown_and_compacted_context() {
        let mut s = Snapshot::demo();
        s.context = None;
        assert!(s.validate().is_ok());
        s.context = Some(ContextUsage {
            tokens: 1000,
            window: 100000,
            percent: 1.0,
        });
        assert!(s.validate().is_ok());
        s.context.as_mut().unwrap().percent = f64::NAN;
        assert!(s.validate().is_err());
    }
}
