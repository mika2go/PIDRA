use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
};

use super::restart::{RestartRequest, RestartResult, execute_restart};

#[derive(Debug)]
pub struct RestartWorker {
    sender: Option<Sender<RestartRequest>>,
    receiver: Receiver<RestartResult>,
    thread: Option<JoinHandle<()>>,
}

impl RestartWorker {
    #[must_use]
    pub fn spawn() -> Self {
        let (sender, request_receiver) = mpsc::channel::<RestartRequest>();
        let (result_sender, receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("pidra-restart".to_owned())
            .spawn(move || {
                while let Ok(request) = request_receiver.recv() {
                    let outcome = execute_restart(&request);
                    if result_sender
                        .send(RestartResult { request, outcome })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to spawn restart worker");
        Self {
            sender: Some(sender),
            receiver,
            thread: Some(thread),
        }
    }

    pub fn request(&self, request: RestartRequest) -> Result<(), String> {
        self.sender
            .as_ref()
            .ok_or_else(|| "restart worker has stopped".to_owned())?
            .send(request)
            .map_err(|_| "restart worker has stopped".to_owned())
    }

    pub fn try_result(&self) -> Option<RestartResult> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for RestartWorker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
