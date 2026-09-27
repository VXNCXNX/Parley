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
    pub fn new() -> Self {
        Self {
            generation: 0,
            active: None,
        }
    }

    pub fn begin(
        &mut self,
        model_path: PathBuf,
        language: Option<String>,
        live: bool,
        preview_tx: Option<Sender<DictationPreview>>,
    ) -> u64 {
        self.cancel();
        self.generation += 1;
        let generation = self.generation;
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            dictation_worker(generation, model_path, language, live, rx, preview_tx)
        });
        self.active = Some(Slot {
            generation,
            tx,
            handle: Some(handle),
            finished: false,
        });
        generation
    }

    pub fn feed(&self, generation: u64, pcm: &[f32]) -> bool {
        let Some(slot) = &self.active else {
            return false;
        };
        if slot.generation != generation || slot.finished {
            return false;
        }
        slot.tx.send(DictationCmd::Feed(pcm.to_vec())).is_ok()
    }

    pub fn finalize(&mut self, generation: u64) -> DictationOutcome {
        let Some(slot) = self.active.as_mut() else {
            return DictationOutcome::Stale;
        };
        if slot.generation != generation || slot.finished {
            return DictationOutcome::Stale;
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        if slot.tx.send(DictationCmd::Finalize(reply_tx)).is_err() {
            return DictationOutcome::Failed("dictation worker stopped".to_string());
        }
        slot.finished = true;
        reply_rx.recv().unwrap_or(DictationOutcome::Failed(
            "dictation worker stopped during finalize".to_string(),
        ))
    }

    pub fn generation(&self) -> u64 {
        self.active
            .as_ref()
            .map(|slot| slot.generation)
            .unwrap_or(0)
    }

    pub fn cancel(&mut self) {
        if let Some(mut slot) = self.active.take() {
            let _ = slot.tx.send(DictationCmd::Cancel);
            if let Some(handle) = slot.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for DictationSlot {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn dictation_worker(
    generation: u64,
    model_path: PathBuf,
    language: Option<String>,
    live: bool,
    rx: Receiver<DictationCmd>,
    preview_tx: Option<Sender<DictationPreview>>,
) {
    let mut dictation = match NativeDictation::begin(&model_path, language.as_deref(), live) {
        Ok(dictation) => Some(dictation),
        Err(error) => {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    DictationCmd::Feed(_) => {}
                    DictationCmd::Finalize(reply) => {
                        let _ = reply.send(DictationOutcome::Failed(error.to_string()));
                        break;
                    }
                    DictationCmd::Cancel => break,
                }
            }
            return;
        }
    };
    let mut feed_error: Option<String> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            DictationCmd::Feed(frame) => {
                if feed_error.is_some() {
                    continue;
                }
                if let Some(dictation) = dictation.as_ref() {
                    match dictation.feed(&frame) {
                        Ok(Some((committed, tentative))) => {
                            if let Some(preview_tx) = &preview_tx {
                                let _ = preview_tx.send(DictationPreview {
                                    generation,
                                    committed,
                                    tentative,
                                });
                            }
                        }
                        Ok(None) => {}
                        Err(error) => feed_error = Some(error.to_string()),
                    }
                }
            }
            DictationCmd::Finalize(reply) => {
                if let Some(error) = feed_error {
                    let _ = reply.send(DictationOutcome::Failed(error));
                    break;
                }
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
            pump_recorder_feed(&rx, |frame| {
                seen_thread.lock().unwrap().extend_from_slice(frame)
            });
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
        slot.cancel();
        assert_eq!(slot.finalize(generation), DictationOutcome::Stale);
    }

    #[test]
    fn cancel_after_native_start_failure_returns() {
        let mut slot = DictationSlot::new();
        let generation = slot.begin(PathBuf::from("invalid\0path"), None, false, None);
        slot.cancel();
        assert_eq!(slot.finalize(generation), DictationOutcome::Stale);
    }

    #[test]
    fn nemotron_streams_french_fixture_to_a_full_final() {
        let Some(root) = std::env::var_os("PARLEY_NATIVE_FIXTURE_DIR").map(PathBuf::from) else {
            return;
        };
        let model = root.join("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf");
        let mut reader = hound::WavReader::open(root.join("french-s16.wav")).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels), (16_000, 1));
        let pcm: Vec<f32> = reader
            .samples::<i16>()
            .map(|sample| sample.unwrap() as f32 / 32768.0)
            .collect();
        let (preview_tx, preview_rx) = mpsc::channel();
        let mut slot = DictationSlot::new();
        let generation = slot.begin(model, Some("fr".to_string()), true, Some(preview_tx));
        for frame in pcm.chunks(480) {
            assert!(slot.feed(generation, frame));
        }
        let result = slot.finalize(generation);
        let DictationOutcome::Final(text) = result else {
            panic!("live transcription failed: {result:?}");
        };
        assert!(text.contains("Bonjour"), "{text}");
        assert!(text.split_whitespace().count() > 8, "{text}");
        assert!(preview_rx.try_iter().count() > 1, "no progressive text");
    }
}
