//! The Android loop: the game with no winit window, drawing into the launcher's surface.
//!
//! winit's event loop on Android is built around a `NativeActivity` and the `AndroidApp`
//! android-activity produces for it. The launcher is an ordinary Activity that owns its own
//! `TextureView`, so the two cannot meet: this loop is what `--headless` is for a client with no
//! display, with the launcher's surface in place of the render target it never opens.
//!
//! Each step is what winit's `about_to_wait` does (`Game::idle`), and `idle` runs the frame
//! through the same `RedrawRequested` path a window's game takes. Input comes from the launcher,
//! queued by the JNI entry points and drained once a step.

use super::Game;
use super::event_loop::EventLoopCtl;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use winit::event_loop::ControlFlow;

/// Time between frames when the game asks for none. The client ticks at 20 Hz whatever the
/// frame rate (a frame runs the ticks its elapsed time owes), so a frame per tick does all the
/// work there is: a paused game needs no more than that.
const FRAME: Duration = Duration::from_millis(17);

/// The pause after a step whose frame cap came from the settings: 260 and up is no cap, and then
/// the swapchain's own wait (`Mailbox`) is the only limiter, as a window's present is.
fn frame_for(limit: i32) -> Duration {
    match limit {
        260.. => Duration::ZERO,
        ..=0 => FRAME,
        _ => Duration::from_secs_f64(1.0 / f64::from(limit)),
    }
}

/// The loop's side of `EventLoopCtl`: what the game asked for during a step.
pub(super) struct SurfaceLoop {
    exit: Cell<bool>,
    flow: Cell<ControlFlow>,
}

impl EventLoopCtl for SurfaceLoop {
    fn exit(&self) {
        crate::android::trace("loop: exit requested");
        self.exit.set(true);
    }
    fn set_control_flow(&self, flow: ControlFlow) {
        self.flow.set(flow);
    }
}

impl SurfaceLoop {
    pub(super) fn new() -> Self {
        Self {
            exit: Cell::new(false),
            flow: Cell::new(ControlFlow::Poll),
        }
    }

    pub(super) fn exited(&self) -> bool {
        self.exit.get()
    }

    /// One step's pause: what the game asked for, else a frame.
    pub(super) fn sleep_a_step(&self) {
        let started = Instant::now();
        if let Some(wait) = self
            .next_step(started)
            .checked_duration_since(Instant::now())
        {
            std::thread::sleep(wait);
        }
    }

    fn next_step(&self, started: Instant) -> Instant {
        match self.flow.get() {
            ControlFlow::WaitUntil(at) => at,
            ControlFlow::Poll | ControlFlow::Wait => started + FRAME,
        }
    }
}

impl Game<'_> {
    /// Runs the game until it asks to stop, drawing into the surface the launcher handed over
    /// (`android::set_external_window`). Ends as a window's game does: `shut_down` saves.
    pub(super) fn run_android_surface(&mut self) {
        let ctl = SurfaceLoop {
            exit: Cell::new(false),
            flow: Cell::new(ControlFlow::Poll),
        };
        self.previous = Instant::now();
        crate::android::trace("loop: first step");
        let mut steps: u64 = 0;
        while !ctl.exit.get() {
            let started = Instant::now();
            ctl.flow.set(ControlFlow::Poll);
            self.idle(&ctl);
            crate::android::note_frame();
            steps += 1;
            if steps == 1 || steps == 120 {
                crate::android::trace(&format!("loop: step {steps}"));
            }
            // The step's pause is the frame cap the settings name, not a frame of its own: a
            // phone that draws faster than 60 needs no sleeping to hold it there.
            let asked = match ctl.flow.get() {
                ControlFlow::WaitUntil(at) => at,
                ControlFlow::Poll | ControlFlow::Wait => {
                    started + frame_for(self.framerate_limit())
                }
            };
            if let Some(wait) = asked.checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        self.shut_down();
    }
}
