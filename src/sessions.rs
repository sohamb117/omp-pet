use crate::model::{Activity, Snapshot};
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Debug)]
pub struct Session {
    pub snapshot: Snapshot,
    pub connection: u64,
    pub connected: bool,
    pub last_activity: Instant,
}

#[derive(Debug, Default)]
pub struct Sessions {
    pub entries: BTreeMap<String, Session>,
    selected: Option<String>,
}

impl Sessions {
    pub fn update(&mut self, connection: u64, snapshot: Snapshot) -> bool {
        if snapshot.validate().is_err() {
            return false;
        }
        if let Some(old) = self.entries.get(&snapshot.session_id) {
            if connection < old.connection
                || (connection == old.connection && snapshot.seq <= old.snapshot.seq)
            {
                return false;
            }
        }
        let now = Instant::now();
        let last_activity = self
            .entries
            .get(&snapshot.session_id)
            .filter(|s| {
                s.snapshot.task == snapshot.task
                    && s.snapshot.tool == snapshot.tool
                    && s.snapshot.activity == snapshot.activity
            })
            .map_or(now, |s| s.last_activity);
        let connected = snapshot.activity != Activity::Disconnected;
        self.entries.insert(
            snapshot.session_id.clone(),
            Session {
                snapshot,
                connection,
                connected,
                last_activity,
            },
        );
        true
    }

    pub fn disconnect(&mut self, connection: u64) {
        for session in self
            .entries
            .values_mut()
            .filter(|s| s.connection == connection)
        {
            session.connected = false;
        }
    }

    pub fn current(&self) -> Option<&Session> {
        if let Some(s) = self.selected.as_ref().and_then(|id| self.entries.get(id)) {
            return Some(s);
        }
        self.entries
            .values()
            .max_by_key(|s| (s.connected, s.snapshot.activity.animates(), s.last_activity))
    }

    pub fn cycle(&mut self) {
        let keys: Vec<_> = self.entries.keys().cloned().collect();
        if keys.is_empty() {
            return;
        }
        let current = self.current().map(|s| &s.snapshot.session_id);
        let index = keys.iter().position(|id| Some(id) == current).unwrap_or(0);
        self.selected = Some(keys[(index + 1) % keys.len()].clone());
    }

    pub fn needs_attention(&self) -> bool {
        self.entries.values().any(|s| {
            s.connected && !matches!(s.snapshot.activity, Activity::Idle | Activity::Disconnected)
        })
    }

    pub fn activity(&self) -> Activity {
        self.current().map_or(Activity::Idle, |s| {
            if s.connected {
                s.snapshot.activity
            } else {
                Activity::Disconnected
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn any_connected_task_keeps_the_pet_awake_even_when_an_idle_session_is_selected() {
        let mut sessions = Sessions::default();
        let mut idle = Snapshot::demo();
        idle.session_id = "idle".into();
        idle.activity = Activity::Idle;
        sessions.update(1, idle);
        sessions.selected = Some("idle".into());
        assert!(!sessions.needs_attention());
        for (seq, activity) in [
            Activity::Working,
            Activity::Waiting,
            Activity::Compacting,
            Activity::Error,
        ]
        .into_iter()
        .enumerate()
        {
            let mut busy = Snapshot::demo();
            busy.seq = seq as u64 + 1;
            busy.activity = activity;
            sessions.update(2, busy);
            assert_eq!(sessions.activity(), Activity::Idle);
            assert!(sessions.needs_attention());
        }
        sessions.disconnect(2);
        assert!(!sessions.needs_attention());
    }
    #[test]
    fn reconnect_accepts_reset_sequence_and_old_socket_cannot_clobber_it() {
        let mut sessions = Sessions::default();
        let mut s = Snapshot::demo();
        s.seq = 20;
        assert!(sessions.update(1, s.clone()));
        s.seq = 1;
        assert!(!sessions.update(1, s.clone()));
        assert!(sessions.update(2, s.clone()));
        sessions.disconnect(1);
        assert!(sessions.current().unwrap().connected);
        s.seq = 30;
        assert!(!sessions.update(1, s));
    }
    #[test]
    fn multiple_sessions_and_disconnect_are_independent() {
        let mut sessions = Sessions::default();
        sessions.update(1, Snapshot::demo());
        let mut s = Snapshot::demo();
        s.session_id = "second".into();
        s.activity = Activity::Idle;
        sessions.update(2, s);
        assert_eq!(sessions.current().unwrap().snapshot.session_id, "demo");
        sessions.disconnect(1);
        assert_eq!(sessions.current().unwrap().snapshot.session_id, "second");
        sessions.cycle();
        assert_eq!(sessions.activity(), Activity::Disconnected);
    }
}
