use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use super::{ControlOutcome, SignalAction, signal::send_signal};
use crate::process::ProcessIdentity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlRequest {
    pub identity: ProcessIdentity,
    pub action: SignalAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlResult {
    pub request: ControlRequest,
    pub outcome: ControlOutcome,
}

#[derive(Debug)]
pub struct ControlWorker {
    sender: Option<Sender<ControlRequest>>,
    receiver: Receiver<ControlResult>,
    thread: Option<JoinHandle<()>>,
}

impl ControlWorker {
    #[must_use]
    pub fn spawn() -> Self {
        let (sender, request_receiver) = mpsc::channel::<ControlRequest>();
        let (result_sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("pidra-control".to_owned())
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    let outcome = send_signal(request.identity, request.action);
                    if result_sender
                        .send(ControlResult { request, outcome })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to spawn process control thread");
        Self {
            sender: Some(sender),
            receiver,
            thread: Some(thread),
        }
    }

    pub fn request(&self, request: ControlRequest) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or_else(|| "control worker has stopped".to_owned())?
            .send(request)
            .map_err(|_| "control worker has stopped".to_owned())
    }

    pub fn try_result(&self) -> Option<ControlResult> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for ControlWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
