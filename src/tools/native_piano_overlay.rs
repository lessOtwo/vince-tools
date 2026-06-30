pub use crate::piano_overlay_protocol::{OverlayFrame, OverlayKeyFrame, OverlayKeyKind};

#[cfg(target_os = "windows")]
mod platform {
    use std::{
        io::{self, BufWriter, Write},
        path::PathBuf,
        process::{Child, ChildStdin, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };

    use crate::piano_overlay_protocol::{OverlayCommand, PIANO_OVERLAY_CHILD_ARG};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    use super::OverlayFrame;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const SPAWN_RETRY_DELAY: Duration = Duration::from_secs(1);

    pub struct NativePianoOverlay {
        child: Option<Child>,
        stdin: Option<BufWriter<ChildStdin>>,
        last_spawn_attempt: Option<Instant>,
        spawn_error_reported: bool,
    }

    impl NativePianoOverlay {
        pub fn new() -> Self {
            Self {
                child: None,
                stdin: None,
                last_spawn_attempt: None,
                spawn_error_reported: false,
            }
        }

        pub fn primary_screen_size() -> (i32, i32) {
            unsafe {
                (
                    GetSystemMetrics(SM_CXSCREEN).max(1),
                    GetSystemMetrics(SM_CYSCREEN).max(1),
                )
            }
        }

        pub fn update(&mut self, frame: &OverlayFrame) {
            if frame.width <= 0 || frame.height <= 0 || frame.opacity <= 0.001 {
                self.hide();
                return;
            }

            if !self.ensure_process() {
                return;
            }

            if self
                .write_command(&OverlayCommand::Frame(frame.clone()))
                .is_err()
            {
                self.clear_process();
            }
        }

        pub fn hide(&mut self) {
            if self.stdin.is_some() && self.write_command(&OverlayCommand::Hide).is_err() {
                self.clear_process();
            }
        }

        fn ensure_process(&mut self) -> bool {
            self.reap_exited_process();
            if self.stdin.is_some() {
                return true;
            }

            if self
                .last_spawn_attempt
                .is_some_and(|attempt| attempt.elapsed() < SPAWN_RETRY_DELAY)
            {
                return false;
            }
            self.last_spawn_attempt = Some(Instant::now());

            match spawn_overlay_process() {
                Ok((child, stdin)) => {
                    self.child = Some(child);
                    self.stdin = Some(BufWriter::new(stdin));
                    self.spawn_error_reported = false;
                    true
                }
                Err(err) => {
                    if !self.spawn_error_reported {
                        eprintln!("piano overlay process failed to start: {err}");
                        self.spawn_error_reported = true;
                    }
                    false
                }
            }
        }

        fn write_command(&mut self, command: &OverlayCommand) -> io::Result<()> {
            let Some(stdin) = self.stdin.as_mut() else {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "piano overlay process is not running",
                ));
            };

            serde_json::to_writer(&mut *stdin, command)?;
            stdin.write_all(b"\n")?;
            stdin.flush()
        }

        fn reap_exited_process(&mut self) {
            let Some(child) = self.child.as_mut() else {
                self.stdin = None;
                return;
            };

            match child.try_wait() {
                Ok(Some(_)) | Err(_) => self.clear_process(),
                Ok(None) => {}
            }
        }

        fn clear_process(&mut self) {
            self.stdin = None;
            self.child = None;
        }
    }

    impl Drop for NativePianoOverlay {
        fn drop(&mut self) {
            if self.stdin.is_some() {
                let _ = self.write_command(&OverlayCommand::Shutdown);
            }
            self.stdin = None;

            let Some(mut child) = self.child.take() else {
                return;
            };

            for _ in 0..10 {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }

            let _ = child.kill();
        }
    }

    fn spawn_overlay_process() -> io::Result<(Child, ChildStdin)> {
        let target = overlay_process_target()?;
        let mut command = Command::new(target.path);
        if target.use_child_arg {
            command.arg(PIANO_OVERLAY_CHILD_ARG);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "failed to open piano overlay process stdin",
            )
        })?;
        Ok((child, stdin))
    }

    fn overlay_process_target() -> io::Result<OverlayProcessTarget> {
        let current_exe = std::env::current_exe()?;
        let sibling = current_exe.with_file_name(overlay_exe_name());
        if sibling.exists() {
            return Ok(OverlayProcessTarget {
                path: sibling,
                use_child_arg: false,
            });
        }

        Ok(OverlayProcessTarget {
            path: current_exe,
            use_child_arg: true,
        })
    }

    fn overlay_exe_name() -> &'static str {
        "piano_overlay.exe"
    }

    struct OverlayProcessTarget {
        path: PathBuf,
        use_child_arg: bool,
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::OverlayFrame;

    pub struct NativePianoOverlay;

    impl NativePianoOverlay {
        pub fn new() -> Self {
            Self
        }

        pub fn primary_screen_size() -> (i32, i32) {
            (1280, 720)
        }

        pub fn update(&mut self, _frame: &OverlayFrame) {}

        pub fn hide(&mut self) {}
    }
}

pub use platform::NativePianoOverlay;
