use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use super::{ProcessSnapshot, procfs};

#[derive(Debug)]
pub struct ScanMessage {
    pub captured_at: SystemTime,
    pub result: Result<Vec<ProcessSnapshot>, String>,
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
        Self::spawn(PathBuf::from("/proc"), refresh_interval)
    }

    #[must_use]
    pub fn spawn(root: PathBuf, refresh_interval: Duration) -> Self {
        let (message_sender, receiver) = mpsc::channel();
        let (stop_sender, stop_receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("pidra-procfs-scanner".to_owned())
            .spawn(move || {
                loop {
                    let result = procfs::scan_procfs(&root).map_err(|error| error.to_string());
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
