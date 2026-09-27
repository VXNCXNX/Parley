use crate::native_runtime::NativeDictation;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DictationPreview {
    pub generation: u64,
    pub committed: String,
    pub tentative: String,
}


pub enum RecorderFeed {
    Frame(Vec<f32>),
    Drained(std::sync::mpsc::Sender<()>),
}

pub fn pump_recorder_feed<F>(rx: &Receiver<RecorderFeed>, mut forward: F)
where
    F: FnMut(&[f32]),
{
    while let Ok(message) = rx.recv() {
        match message {
            RecorderFeed::Frame(frame) => forward(&frame),
            RecorderFeed::Drained(reply) => {
                let _ = reply.send(());
                break;
            }
        }
    }
}
enum DictationCmd {
    Feed(Vec<f32>),
    Barrier(Sender<()>),
    Finalize(Sender<DictationOutcome>),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationOutcome {
    Final(String),
    Empty,
    Failed(String),
    Stale,
}

struct Slot {
    generation: u64,
    tx: Sender<DictationCmd>,
    handle: Option<JoinHandle<()>>,
    finished: bool,
}

pub struct DictationSlot {
    generation: u64,
    active: Option<Slot>,
}

impl DictationSlot {
    pub fn new() -> Self { Self { generation: 0, active: None } }

    pub fn begin(&mut self, model_path: PathBuf, language: Option<String>, live: bool, preview_tx: Option<Sender<DictationPreview>>) -> u64 {
        self.cancel();
        self.generation += 1;
        let generation = self.generation;
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || dictation_worker(generation, model_path, language, live, rx, preview_tx));
        self.active = Some(Slot { generation, tx, handle: Some(handle), finished: false });
        generation
    }

    pub fn wait_for_queued_audio(&self) {
        let Some(slot) = &self.active else { return };
        if slot.finished { return };
        let (reply_tx, reply_rx) = mpsc::channel();
        if slot.tx.send(DictationCmd::Barrier(reply_tx)).is_ok() { let _ = reply_rx.recv(); }
    }

    pub fn feed(&self, generation: u64, pcm: &[f32]) -> bool {
        let Some(slot) = &self.active else { return false };
        if slot.generation != generation || slot.finished { return false; }
        slot.tx.send(DictationCmd::Feed(pcm.to_vec())).is_ok()
    }

    pub fn finalize(&mut self, generation: u64) -> DictationOutcome {
        let Some(slot) = self.active.as_mut() else { return DictationOutcome::Stale };
        if slot.generation != generation || slot.finished { return DictationOutcome::Stale; }
        let (reply_tx, reply_rx) = mpsc::channel();
        if slot.tx.send(DictationCmd::Finalize(reply_tx)).is_err() { return DictationOutcome::Failed("dictation worker stopped".to_string()); }
        slot.finished = true;
        reply_rx.recv().unwrap_or(DictationOutcome::Failed("dictation worker stopped during finalize".to_string()))
    }

    pub fn generation(&self) -> u64 { self.active.as_ref().map(|slot| slot.generation).unwrap_or(0) }

    pub fn cancel(&mut self) {
        if let Some(mut slot) = self.active.take() {
            let _ = slot.tx.send(DictationCmd::Cancel);
            if let Some(handle) = slot.handle.take() { let _ = handle.join(); }
        }
    }
}

impl Drop for DictationSlot { fn drop(&mut self) { self.cancel(); } }

fn dictation_worker(generation: u64, model_path: PathBuf, language: Option<String>, live: bool, rx: Receiver<DictationCmd>, preview_tx: Option<Sender<DictationPreview>>) {
    let mut dictation = match NativeDictation::begin(&model_path, language.as_deref(), live) {
        Ok(dictation) => Some(dictation),
        Err(error) => {
            while let Ok(cmd) = rx.recv() {
                if let DictationCmd::Finalize(reply) = cmd { let _ = reply.send(DictationOutcome::Failed(error.to_string())); break; }
            }
            return;
        }
    };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            DictationCmd::Barrier(reply) => { let _ = reply.send(()); }
            DictationCmd::Feed(frame) => {
                if let Some(dictation) = dictation.as_ref() {
                    if let Ok(Some((committed, tentative))) = dictation.feed(&frame) {
                        if let Some(preview_tx) = &preview_tx {
                            let _ = preview_tx.send(DictationPreview { generation, committed, tentative });
                        }
                    }
                }
            }
            DictationCmd::Finalize(reply) => {
                let outcome = match dictation.as_mut() {
                    Some(dictation) => match dictation.finalize() {
                        Ok(text) if text.trim().is_empty() => DictationOutcome::Empty,
                        Ok(text) => DictationOutcome::Final(text),
                        Err(error) => DictationOutcome::Failed(error.to_string()),
                    },
                    None => DictationOutcome::Failed("native session missing".to_string()),
                };
                let _ = reply.send(outcome);
                break;
            }
            DictationCmd::Cancel => break,
        }
    }
}

pub fn stale_after_cancel(slot: &mut DictationSlot, generation: u64) -> DictationOutcome {
    slot.cancel();
    slot.finalize(generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_forwarding_keeps_queued_audio() {
        let (tx, rx) = mpsc::channel();
        tx.send(RecorderFeed::Frame(vec![1.0, 2.0])).unwrap();
        tx.send(RecorderFeed::Frame(vec![3.0])).unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        tx.send(RecorderFeed::Drained(done_tx)).unwrap();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
        let gate_thread = std::sync::Arc::clone(&gate);
        let seen_thread = std::sync::Arc::clone(&seen);
        let handle = thread::spawn(move || {
            gate_thread.wait();
            pump_recorder_feed(&rx, |frame| seen_thread.lock().unwrap().extend_from_slice(frame));
        });
        assert!(done_rx.try_recv().is_err());
        gate.wait();
        handle.join().unwrap();
        done_rx.recv().unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn second_finalize_is_stale() {
        let mut slot = DictationSlot::new();
        let generation = slot.begin(PathBuf::from("missing.gguf"), None, false, None);
        let first = slot.finalize(generation);
        let second = slot.finalize(generation);
        assert!(matches!(first, DictationOutcome::Failed(_)));
        assert_eq!(second, DictationOutcome::Stale);
    }

    #[test]
    fn cancel_blocks_later_finalize() {
        let mut slot = DictationSlot::new();
        let generation = slot.begin(PathBuf::from("missing.gguf"), None, false, None);
        assert_eq!(stale_after_cancel(&mut slot, generation), DictationOutcome::Stale);
    }
}
