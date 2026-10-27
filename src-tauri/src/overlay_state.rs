//! Request lifetime independent of windows, events and the renderer.
use std::time::{Duration, Instant};
pub const DEADLINE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Queued,
    Received,
    Mounted,
    Shown,
    Closed,
    Failed,
}
impl Phase {
    pub fn pending(self) -> bool {
        matches!(self, Self::Queued | Self::Received | Self::Mounted)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Received => "received",
            Self::Mounted => "mounted",
            Self::Shown => "shown",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Failure {
    pub stage: String,
    pub message: String,
    pub infrastructure: bool,
}
#[derive(Clone)]
pub struct Request<T> {
    pub id: u32,
    pub value: T,
    pub phase: Phase,
    pub started: Instant,
    pub failure: Option<Failure>,
}
pub struct State<T> {
    pub created_at: Instant,
    pub ready: bool,
    pub disabled: Option<Failure>,
    pub request: Option<Request<T>>,
    highest_id: u32,
}
impl<T> Default for State<T> {
    fn default() -> Self {
        Self {
            created_at: Instant::now(),
            ready: false,
            disabled: None,
            request: None,
            highest_id: 0,
        }
    }
}
impl<T> State<T> {
    pub fn submit(&mut self, id: u32, value: T) -> Result<(), String> {
        if self.disabled.is_some() {
            return Err("悬浮层本次会话已停用。".into());
        }
        if id <= self.highest_id
            || self
                .request
                .as_ref()
                .is_some_and(|r| r.phase.pending() || r.phase == Phase::Shown)
        {
            return Err("悬浮层请求已过期或正在处理请求。".into());
        }
        self.highest_id = id;
        self.request = Some(Request {
            id,
            value,
            phase: Phase::Queued,
            started: Instant::now(),
            failure: None,
        });
        Ok(())
    }
    pub fn advance(&mut self, id: u32, phase: Phase) -> Result<(), String> {
        self.expire();
        if self.disabled.is_some() {
            return Err("悬浮层本次会话已停用。".into());
        }
        let r = self
            .request
            .as_mut()
            .filter(|r| r.id == id && r.phase.pending())
            .ok_or("悬浮层请求已过期。")?;
        let valid = matches!(
            (r.phase, phase),
            (Phase::Queued, Phase::Received)
                | (Phase::Received, Phase::Received)
                | (Phase::Received, Phase::Mounted)
                | (Phase::Mounted, Phase::Shown)
        );
        if !valid {
            return Err("悬浮层呈现顺序无效。".into());
        }
        r.phase = phase;
        Ok(())
    }
    pub fn cancel(&mut self, id: u32) -> bool {
        // A cancellation arriving before submit also invalidates a late submit.
        self.highest_id = self.highest_id.max(id);
        if let Some(r) = self.request.as_mut().filter(|r| r.id == id) {
            if r.phase != Phase::Failed {
                r.phase = Phase::Closed;
            }
            true
        } else {
            false
        }
    }
    pub fn fail(&mut self, id: Option<u32>, failure: Failure) -> bool {
        let active = |r: &Request<T>| r.phase.pending() || r.phase == Phase::Shown;
        if let Some(id) = id {
            if let Some(r) = self.request.as_mut().filter(|r| r.id == id && active(r)) {
                r.phase = Phase::Failed;
                r.failure = Some(failure.clone());
            } else if failure.infrastructure
                && id > self.highest_id
                && !self.request.as_ref().is_some_and(active)
            {
                // IPC may fail before submission arrives. Invalidate it too.
                self.highest_id = id;
            } else {
                return false;
            }
        } else if failure.infrastructure {
            // Bootstrap/runtime failures may have no renderer request ID yet.
            if let Some(r) = self.request.as_mut().filter(|r| active(r)) {
                r.phase = Phase::Failed;
                r.failure = Some(failure.clone());
            }
        } else {
            return false;
        }
        if failure.infrastructure {
            self.disabled = Some(failure);
        }
        true
    }
    pub fn expire(&mut self) {
        if let Some(r) = self
            .request
            .as_ref()
            .filter(|r| r.phase.pending() && r.started.elapsed() >= DEADLINE)
        {
            let stage = if !self.ready {
                "initialization"
            } else {
                r.phase.as_str()
            };
            self.fail(
                Some(r.id),
                Failure {
                    stage: stage.into(),
                    message: "悬浮层无响应，已切换为本地弹窗。".into(),
                    infrastructure: true,
                },
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_rejects_late_read_mount_show_and_submit() {
        let mut s = State::default();
        s.submit(1, ()).unwrap();
        s.advance(1, Phase::Received).unwrap();
        s.cancel(1);
        assert!(s.advance(1, Phase::Mounted).is_err());
        assert!(s.advance(1, Phase::Shown).is_err());
        s.cancel(3);
        assert!(s.submit(3, ()).is_err());
        let mut initialization = State::default();
        initialization.submit(1, ()).unwrap();
        initialization.fail(
            None,
            Failure {
                stage: "initialization".into(),
                message: "failed".into(),
                infrastructure: true,
            },
        );
        assert!(initialization.advance(1, Phase::Received).is_err());
        assert_eq!(
            initialization.request.as_ref().unwrap().phase,
            Phase::Failed
        );
        let mut early: State<()> = State::default();
        assert!(early.fail(
            Some(1),
            Failure {
                stage: "ipc".into(),
                message: "failed".into(),
                infrastructure: true
            }
        ));
        assert!(early.submit(1, ()).is_err());

        s.submit(4, ()).unwrap();
        assert!(!s.cancel(1));
        assert!(s.submit(5, ()).is_err());
    }
    #[test]
    fn normal_handshake_reuses_window_and_ignores_stale_confirmation() {
        let mut s = State::default();
        s.ready = true;
        s.submit(1, "settings").unwrap();
        assert!(s.advance(1, Phase::Shown).is_err());
        for phase in [
            Phase::Received,
            Phase::Received,
            Phase::Mounted,
            Phase::Shown,
        ] {
            s.advance(1, phase).unwrap();
        }
        s.cancel(1);
        s.submit(2, "help").unwrap();
        assert!(s.advance(1, Phase::Received).is_err());
        assert_eq!(s.request.as_ref().unwrap().value, "help");
    }
    #[test]
    fn business_failure_recovers_but_timeout_disables_session() {
        let mut s = State::default();
        s.submit(1, ()).unwrap();
        assert!(s.fail(
            Some(1),
            Failure {
                stage: "content".into(),
                message: "removed".into(),
                infrastructure: false
            }
        ));
        s.submit(2, ()).unwrap();
        s.request.as_mut().unwrap().started = Instant::now() - DEADLINE;
        assert!(s.advance(2, Phase::Received).is_err());
        assert!(s.disabled.is_some());
        assert!(!s.fail(
            Some(1),
            Failure {
                stage: "late".into(),
                message: "late".into(),
                infrastructure: true
            }
        ));
        assert!(s.submit(3, ()).is_err());
    }
}
