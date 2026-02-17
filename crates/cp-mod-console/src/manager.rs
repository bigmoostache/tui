use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cp_base::constants::STORE_DIR;
use cp_base::panels::now_ms;

use crate::ring_buffer::RingBuffer;
use crate::types::ProcessStatus;
use crate::CONSOLE_DIR;

/// A managed child process session.
/// All fields are Send + Sync via Arc wrappers.
pub struct SessionHandle {
    pub name: String,
    pub command: String,
    pub cwd: Option<String>,
    pub status: Arc<Mutex<ProcessStatus>>,
    pub buffer: RingBuffer,
    pub log_path: String,
    pub input_path: String,
    child_id: Arc<Mutex<Option<u32>>>,
    pub started_at: u64,
    pub finished_at: Arc<Mutex<Option<u64>>>,
    stop_polling: Arc<AtomicBool>,
}

// Safety: All Arc<Mutex<_>> fields are Send+Sync.
// RingBuffer is Clone via Arc<Mutex<_>> so also Send+Sync.
unsafe impl Send for SessionHandle {}
unsafe impl Sync for SessionHandle {}

/// Build the log file path for a given session key (always absolute).
pub fn log_file_path(key: &str) -> PathBuf {
    let base = PathBuf::from(STORE_DIR).join(CONSOLE_DIR).join(format!("{}.log", key));
    // Ensure absolute path so cwd doesn't break file resolution
    if base.is_absolute() { base } else { std::env::current_dir().unwrap_or_default().join(base) }
}

/// Build the input file path for a given session key (always absolute).
pub fn input_file_path(key: &str) -> PathBuf {
    let base = PathBuf::from(STORE_DIR).join(CONSOLE_DIR).join(format!("{}.in", key));
    if base.is_absolute() { base } else { std::env::current_dir().unwrap_or_default().join(base) }
}

impl SessionHandle {
    /// Spawn a new child process with file-based I/O.
    ///
    /// Stdin uses `tail -f {input_file} | command` so the process survives parent death.
    /// Stdout/stderr redirect to `{log_file}` and are polled into the ring buffer.
    pub fn spawn(name: String, command: String, cwd: Option<String>) -> Result<Self, String> {
        // Ensure console directory exists
        let console_dir = PathBuf::from(STORE_DIR).join(CONSOLE_DIR);
        fs::create_dir_all(&console_dir).map_err(|e| format!("Failed to create console dir: {}", e))?;

        let log_path = log_file_path(&name);
        let log_path_str = log_path.to_string_lossy().to_string();
        let input_path = input_file_path(&name);
        let input_path_str = input_path.to_string_lossy().to_string();

        // Create/truncate both files
        fs::write(&log_path, b"").map_err(|e| format!("Failed to create log file: {}", e))?;
        fs::write(&input_path, b"").map_err(|e| format!("Failed to create input file: {}", e))?;

        // Wrap command: tail -f feeds stdin from file, output redirected to log
        let wrapped_cmd = format!(
            "tail -f {} | ({}) >> {} 2>&1",
            input_path_str, command, log_path_str
        );

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", &wrapped_cmd]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", &wrapped_cmd]);
            c
        };

        // All I/O handled by the wrapper — no pipes needed
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

        if let Some(ref dir) = cwd {
            cmd.current_dir(dir);
        }

        // Detach child into its own process group so it survives parent death
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let child = cmd.spawn().map_err(|e| format!("Failed to spawn '{}': {}", command, e))?;

        let pid = child.id();

        let status = Arc::new(Mutex::new(ProcessStatus::Running));
        let buffer = RingBuffer::new();
        let child_id = Arc::new(Mutex::new(Some(pid)));
        let finished_at = Arc::new(Mutex::new(None));
        let stop_polling = Arc::new(AtomicBool::new(false));

        // File poller thread: reads new bytes from log file into ring buffer
        {
            let buf = buffer.clone();
            let stop = Arc::clone(&stop_polling);
            let path = log_path.clone();
            std::thread::spawn(move || {
                file_poller(path, buf, stop);
            });
        }

        // Waiter thread: wait for child to exit
        {
            let status_clone = Arc::clone(&status);
            let finished_clone = Arc::clone(&finished_at);
            let stop_clone = Arc::clone(&stop_polling);
            std::thread::spawn(move || {
                wait_for_child(child, status_clone, finished_clone, stop_clone);
            });
        }

        Ok(Self {
            name,
            command,
            cwd,
            status,
            buffer,
            log_path: log_path_str,
            input_path: input_path_str,
            child_id,
            started_at: now_ms(),
            finished_at,
            stop_polling,
        })
    }

    /// Reconnect to a previously-running session after TUI reload.
    /// Loads log file contents into ring buffer and monitors the PID.
    /// `send_input` works identically to fresh sessions (appends to .in file).
    pub fn reconnect(
        name: String,
        command: String,
        cwd: Option<String>,
        pid: u32,
        log_path_str: String,
        input_path_str: String,
        started_at: u64,
    ) -> Self {
        let log_path = PathBuf::from(&log_path_str);
        let status = Arc::new(Mutex::new(ProcessStatus::Running));
        let buffer = RingBuffer::new();
        let child_id = Arc::new(Mutex::new(Some(pid)));
        let finished_at = Arc::new(Mutex::new(None));
        let stop_polling = Arc::new(AtomicBool::new(false));

        // Load existing log file contents into ring buffer
        let file_offset = if let Ok(content) = fs::read(&log_path) {
            if !content.is_empty() {
                buffer.write(&content);
            }
            content.len() as u64
        } else {
            0
        };

        // Check if PID is still alive
        let pid_alive = is_pid_alive(pid);

        if pid_alive {
            // Start file poller at current offset
            {
                let buf = buffer.clone();
                let stop = Arc::clone(&stop_polling);
                let path = log_path.clone();
                std::thread::spawn(move || {
                    file_poller_from_offset(path, buf, stop, file_offset);
                });
            }

            // Start PID monitor thread
            {
                let status_clone = Arc::clone(&status);
                let finished_clone = Arc::clone(&finished_at);
                let stop_clone = Arc::clone(&stop_polling);
                std::thread::spawn(move || {
                    wait_for_pid(pid, status_clone, finished_clone, stop_clone);
                });
            }
        } else {
            // Process already dead
            let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
            *s = ProcessStatus::Finished(-1);
            let mut fin = finished_at.lock().unwrap_or_else(|e| e.into_inner());
            *fin = Some(now_ms());
            stop_polling.store(true, Ordering::Relaxed);
        }

        Self {
            name,
            command,
            cwd,
            status,
            buffer,
            log_path: log_path_str,
            input_path: input_path_str,
            child_id,
            started_at,
            finished_at,
            stop_polling,
        }
    }

    /// Send input to the process by appending to the .in file.
    /// Works identically for fresh and reconnected sessions.
    pub fn send_input(&self, input: &str) -> Result<(), String> {
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&self.input_path)
            .map_err(|e| format!("stdin write failed: {}", e))?;
        f.write_all(input.as_bytes()).map_err(|e| format!("stdin write failed: {}", e))?;
        Ok(())
    }

    /// Kill the process (and its process group).
    pub fn kill(&self) {
        // Signal polling thread to stop
        self.stop_polling.store(true, Ordering::Relaxed);

        let pid = {
            let guard = self.child_id.lock().unwrap_or_else(|e| e.into_inner());
            *guard
        };
        if let Some(pid) = pid {
            if cfg!(target_os = "windows") {
                let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output();
            } else {
                // Kill the entire process group (negative PID).
                // process_group(0) makes PID == PGID.
                let _ = Command::new("kill").args(["-9", &format!("-{}", pid)]).output();
            }
        }
        // Update status
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if !status.is_terminal() {
            *status = ProcessStatus::Killed;
        }
        let mut fin = self.finished_at.lock().unwrap_or_else(|e| e.into_inner());
        if fin.is_none() {
            *fin = Some(now_ms());
        }
    }

    /// Get the current process status.
    pub fn get_status(&self) -> ProcessStatus {
        self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get exit code (if process is terminal).
    pub fn exit_code(&self) -> Option<i32> {
        self.get_status().exit_code()
    }

    /// Get the PID if available.
    pub fn pid(&self) -> Option<u32> {
        *self.child_id.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Check if a PID is alive via `kill -0`.
fn is_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// File poller: reads new bytes from a log file into a ring buffer.
fn file_poller(path: PathBuf, buffer: RingBuffer, stop: Arc<AtomicBool>) {
    file_poller_from_offset(path, buffer, stop, 0);
}

/// File poller starting from a given offset.
fn file_poller_from_offset(path: PathBuf, buffer: RingBuffer, stop: Arc<AtomicBool>, mut offset: u64) {
    use std::io::{Read, Seek, SeekFrom};

    loop {
        if stop.load(Ordering::Relaxed) {
            // Grace period: read any final bytes after process exit
            std::thread::sleep(std::time::Duration::from_millis(300));
            if let Ok(mut f) = fs::File::open(&path)
                && f.seek(SeekFrom::Start(offset)).is_ok()
            {
                let mut buf = vec![0u8; 64 * 1024];
                while let Ok(n) = f.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    buffer.write(&buf[..n]);
                }
            }
            break;
        }

        if let Ok(mut f) = fs::File::open(&path)
            && f.seek(SeekFrom::Start(offset)).is_ok()
        {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        buffer.write(&buf[..n]);
                        offset += n as u64;
                    }
                    Err(_) => break,
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Background thread: wait for child to exit and update status.
fn wait_for_child(
    mut child: Child,
    status: Arc<Mutex<ProcessStatus>>,
    finished_at: Arc<Mutex<Option<u64>>>,
    stop_polling: Arc<AtomicBool>,
) {
    match child.wait() {
        Ok(exit_status) => {
            let code = exit_status.code().unwrap_or(-1);
            let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
            if !s.is_terminal() {
                *s = if code == 0 { ProcessStatus::Finished(code) } else { ProcessStatus::Failed(code) };
            }
        }
        Err(_) => {
            let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
            if !s.is_terminal() {
                *s = ProcessStatus::Failed(-1);
            }
        }
    }
    let mut fin = finished_at.lock().unwrap_or_else(|e| e.into_inner());
    if fin.is_none() {
        *fin = Some(now_ms());
    }
    // Signal file poller to do final read and stop
    stop_polling.store(true, Ordering::Relaxed);
}

/// Background thread: poll `kill -0` to detect when a PID exits (used for reconnected sessions).
fn wait_for_pid(
    pid: u32,
    status: Arc<Mutex<ProcessStatus>>,
    finished_at: Arc<Mutex<Option<u64>>>,
    stop_polling: Arc<AtomicBool>,
) {
    loop {
        if stop_polling.load(Ordering::Relaxed) {
            break;
        }
        if !is_pid_alive(pid) {
            let mut s = status.lock().unwrap_or_else(|e| e.into_inner());
            if !s.is_terminal() {
                *s = ProcessStatus::Finished(-1);
            }
            let mut fin = finished_at.lock().unwrap_or_else(|e| e.into_inner());
            if fin.is_none() {
                *fin = Some(now_ms());
            }
            stop_polling.store(true, Ordering::Relaxed);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
