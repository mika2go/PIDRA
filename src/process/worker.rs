use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use super::{
    GuiClassification, ProcessSnapshot,
    gui::{classify_gui_processes, discover_window_hints},
    procfs,
};

#[derive(Debug)]
pub struct ScanBatch {
    pub processes: Vec<ProcessSnapshot>,
    pub graphical: Vec<GuiClassification>,
}

#[derive(Debug)]
pub struct ScanMessage {
    pub captured_at: SystemTime,
    pub result: Result<ScanBatch, String>,
}

#[derive(Debug)]
pub struct ScanWorker {
    receiver: Receiver<ScanMessage>,
    stop_sender: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl ScanWorker {
    #[must_use]
    pub fn spawn_system(refresh_interval: Duration) -> Self {
        Self::spawn_with_gui_detection(PathBuf::from("/proc"), refresh_interval, true)
    }

    #[must_use]
    pub fn spawn(root: PathBuf, refresh_interval: Duration) -> Self {
        Self::spawn_with_gui_detection(root, refresh_interval, false)
    }

    fn spawn_with_gui_detection(
        root: PathBuf,
        refresh_interval: Duration,
        detect_windows: bool,
    ) -> Self {
        let (message_sender, receiver) = mpsc::channel();
        let (stop_sender, stop_receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("pidra-procfs-scanner".to_owned())
            .spawn(move || {
                loop {
                    let result = procfs::scan_procfs(&root)
                        .map(|processes| {
                            let window_hints = if detect_windows {
                                discover_window_hints()
                            } else {
                                Vec::new()
                            };
                            let graphical = classify_gui_processes(&processes, &window_hints);
                            ScanBatch {
                                processes,
                                graphical,
                            }
                        })
                        .map_err(|error| error.to_string());
                    if message_sender
                        .send(ScanMessage {
                            captured_at: SystemTime::now(),
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                    if stop_receiver.recv_timeout(refresh_interval).is_ok() {
                        break;
                    }
                }
            })
            .expect("failed to spawn procfs scanner thread");

        Self {
            receiver,
            stop_sender,
            thread: Some(thread),
        }
    }

    pub fn try_latest(&self) -> Option<ScanMessage> {
        let mut latest = None;
        loop {
            match self.receiver.try_recv() {
                Ok(message) => latest = Some(message),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return latest,
            }
        }
    }
}

impl Drop for ScanWorker {
    fn drop(&mut self) {
        let _ = self.stop_sender.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
