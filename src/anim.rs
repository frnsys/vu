use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use winit::event_loop::EventLoop;

/// Event emitted when the next frame in
/// an image sequence should be shown.
#[derive(Debug)]
pub struct RequestNextFrame;

pub struct Animator {
    handle: Option<JoinHandle<()>>,

    /// A flag indicating when the frame delay thread
    /// should terminate.
    is_running: Arc<AtomicBool>,
}
impl Animator {
    pub fn new(event_loop: &EventLoop<RequestNextFrame>, delays: &[f64]) -> Self {
        let is_running = Arc::new(AtomicBool::new(true));

        // Setup a separate thread to handle frame
        // delays/advancement.
        let should_run = Arc::clone(&is_running);
        let proxy = event_loop.create_proxy();
        let delays = delays.to_vec();
        let handle = thread::spawn(move || {
            'outer: while should_run.load(Ordering::SeqCst) {
                for delay in &delays {
                    thread::sleep(Duration::from_secs_f64(*delay));
                    if proxy.send_event(RequestNextFrame).is_err() {
                        break 'outer;
                    }
                }
            }
        });

        Self {
            is_running,
            handle: Some(handle),
        }
    }
}
impl Drop for Animator {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.handle.take().unwrap().join().unwrap();
    }
}
