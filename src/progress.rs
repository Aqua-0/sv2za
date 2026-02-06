use std::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    PhaseStart { name: String },
    Progress { done: u64, total: u64 },
    Info { msg: String },
    Warn { msg: String },
    Error { msg: String },
    PhaseEnd { name: String },
    Finished { ok: bool },
}

#[derive(Clone)]
pub struct ProgressSink {
    tx: mpsc::Sender<ProgressEvent>,
}

impl ProgressSink {
    pub fn new() -> (Self, mpsc::Receiver<ProgressEvent>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx }, rx)
    }

    pub fn send(&self, ev: ProgressEvent) {
        let _ = self.tx.send(ev);
    }

    pub fn phase_start(&self, name: impl Into<String>) {
        self.send(ProgressEvent::PhaseStart { name: name.into() });
    }

    pub fn phase_end(&self, name: impl Into<String>) {
        self.send(ProgressEvent::PhaseEnd { name: name.into() });
    }

    pub fn progress(&self, done: u64, total: u64) {
        self.send(ProgressEvent::Progress { done, total });
    }

    pub fn info(&self, msg: impl Into<String>) {
        self.send(ProgressEvent::Info { msg: msg.into() });
    }

    pub fn warn(&self, msg: impl Into<String>) {
        self.send(ProgressEvent::Warn { msg: msg.into() });
    }

    pub fn error(&self, msg: impl Into<String>) {
        self.send(ProgressEvent::Error { msg: msg.into() });
    }

    pub fn finished(&self, ok: bool) {
        self.send(ProgressEvent::Finished { ok });
    }
}
