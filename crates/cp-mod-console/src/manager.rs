use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cp_base::config::STORE_DIR;
use cp_base::panels::now_ms;

use crate::ring_buffer::RingBuffer;
use crate::types::ProcessStatus;
use crate::CONSOLE_DIR;

/// Environment variable set on all spawned processes to tag them as TUI-launched.
/// Value is a hash of the working directory so multiple TUI instances don't collide.
pub const ENV_TAG: &str = "CONTEXT_PILOT_SESSION";

/// Compute the session tag for the current working directory.
pub fn session_tag() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    cwd.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Kill orphaned processes from previous TUI sessions that weren't cleaned up.
/// Scans /proc for processes with our CONTEXT_PILOT_SESSION env var and kills
/// any that aren't in the provided set of known PIDs.
pub fn kill_orphaned_processes(known_pids: &std::collections::HashSet<u32>) {
    let tag = session_tag();

    // Scan /proc/*/environ for our tag
    let proc_dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return, // Not on Linux or no /proc access
    };

    for entry in proc_dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only look at numeric dirs (PIDs)
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Skip our own PID and known live sessions
        if pid == std::process::id() || known_pids.contains(&pid) {
            continue;
        }

        // Read environ (null-separated key=value pairs)
        let environ_path = format!("/proc/{}/environ", pid);
        let environ = match std::fs::read(&environ_path) {
            Ok(data) => data,
            Err(_) => continue, // Permission denied or process gone
        };

        // Check if our tag is present
        let needle = format!("{}={}", ENV_TAG, tag);
        let needle_bytes = needle.as_bytes();

        let found = environ
            .split(|&b| b == 0)
            .any(|entry| entry == needle_bytes);

        if found {
            // Orphan found — kill its process group
            let _ = Command::new("kill").args(["-9", &format!("-{}", pid)]).output();
            // Fallback: kill just the PID if group kill fails
            let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
        }
    }
}

/// A managed child process session.
/// All fields are Send + Sync via Arc wrappers.
pub struct SessionHandle {
    pub name: String,
    pub command: String,
    pub cwd: Option<String>,
    pub status: Arc<Mutex<ProcessStatus>>,
    pub buffer: RingBuffer,
    pub log_path: String,
    /// Piped stdin handle — None after reconnect (pipe lost).
    stdin_handle: Arc<Mutex<Option<std::process::ChildStdin>>>,
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

impl SessionHandle {
    /// Spawn a new child process using `script` for PTY emulation.
    ///
    /// Uses `script -q -f -c` to create a PTY so the child sees a real terminal.
    /// Output is captured to a log file and polled into the ring buffer.
    /// Stdin is piped directly to `script` which forwards to the PTY.
    pub fn spawn(name: String, command: String, cwd: Option<String>) -> Result<Self, String> {
        // Ensure console directory exists
        let console_dir = PathBuf::from(STORE_DIR).join(CONSOLE_DIR);
        fs::create_dir_all(&console_dir).map_err(|e| format!("Failed to create console dir: {}", e))?;

        let log_path = log_file_path(&name);
        let log_path_str = log_path.to_string_lossy().to_string();

        // Create/truncate log file
        fs::write(&log_path, b"").map_err(|e| format!("Failed to create log file: {}", e))?;

        // Use `script` for PTY emulation:
        //   -q: quiet (no "Script started" header)
        //   -f: flush after each write (real-time output)
        //   -c: run command instead of interactive shell
        let mut cmd = Command::new("script");
        cmd.args(["-q", "-f", "-c", &command, &log_path_str]);

        // Tag the process so we can find orphans after crash/SIGKILL
        cmd.env(ENV_TAG, session_tag());

        // Pipe stdin so we can send input via send_keys()
        // Stdout/stderr null — all output captured by script to the log file
        cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

        if let Some(ref dir) = cwd {
            cmd.current_dir(dir);
        }

        // Detach child into its own process group so it survives parent death
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn '{}': {}", command, e))?;

        let pid = child.id();
        let stdin_handle = child.stdin.take();

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
            stdin_handle: Arc::new(Mutex::new(stdin_handle)),
            child_id,
            started_at: now_ms(),
            finished_at,
            stop_polling,
        })
    }

    /// Reconnect to a previously-running session after TUI reload.
    /// Loads log file contents into ring buffer and monitors the PID.
    /// `send_input` will fail — stdin pipe is lost after reload.
    pub fn reconnect(
        name: String,
        command: String,
        cwd: Option<String>,
        pid: u32,
        log_path_str: String,
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
            stdin_handle: Arc::new(Mutex::new(None)), // No stdin after reconnect
            child_id,
            started_at,
            finished_at,
            stop_polling,
        }
    }

    /// Send input to the process via the piped stdin.
    /// Escape sequences are interpreted before sending:
    ///   \n \r \t \\ → standard escapes
    ///   \xHH        → arbitrary hex byte (e.g. \x03 for Ctrl+C)
    ///   \e          → ESC (0x1B), for ANSI sequences like \e[A (up arrow)
    /// Fails after reconnect (stdin pipe is lost).
    pub fn send_input(&self, input: &str) -> Result<(), String> {
        let bytes = interpret_escapes(input);
        let mut guard = self.stdin_handle.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut stdin) = *guard {
            stdin.write_all(&bytes).map_err(|e| format!("stdin write failed: {}", e))?;
            stdin.flush().map_err(|e| format!("stdin flush failed: {}", e))?;
            Ok(())
        } else {
            Err("stdin unavailable (reconnected session — stdin pipe lost after reload)".to_string())
        }
    }

    /// Kill the process.
    ///
    /// Sends SIGTERM to the `script` process only (not the process group).
    /// When `script` dies, its PTY master fd closes, which sends SIGHUP to
    /// all child processes via the PTY — this is the clean shutdown path.
    /// Falls back to SIGKILL after a brief delay if SIGTERM doesn't work.
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
                // Send SIGTERM to just the script process (NOT the group).
                // script's PTY cleanup will propagate SIGHUP to children.
                let _ = Command::new("kill").args([&pid.to_string()]).output();
                // Brief wait, then SIGKILL if still alive
                std::thread::sleep(std::time::Duration::from_millis(100));
                if is_pid_alive(pid) {
                    let _ = Command::new("kill").args(["-9", &pid.to_string()]).output();
                }
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

/// Interpret escape sequences in input text, converting them to raw bytes.
///
/// Supported sequences:
///   \n      → 0x0A (newline)
///   \r      → 0x0D (carriage return)
///   \t      → 0x09 (tab)
///   \\      → 0x5C (literal backslash)
///   \e      → 0x1B (ESC, for ANSI sequences like \e[A)
///   \xHH    → arbitrary hex byte (e.g. \x03 = Ctrl+C, \x04 = Ctrl+D)
///   \0      → 0x00 (null)
///
/// Unrecognized sequences pass through as-is (backslash + char).
fn interpret_escapes(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' => {
                    out.push(0x0A);
                    i += 2;
                }
                b'r' => {
                    out.push(0x0D);
                    i += 2;
                }
                b't' => {
                    out.push(0x09);
                    i += 2;
                }
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'e' => {
                    out.push(0x1B);
                    i += 2;
                }
                b'0' => {
                    out.push(0x00);
                    i += 2;
                }
                b'x' if i + 3 < bytes.len() => {
                    // Parse \xHH
                    let hi = bytes[i + 2];
                    let lo = bytes[i + 3];
                    if let (Some(h), Some(l)) = (hex_digit(hi), hex_digit(lo)) {
                        out.push(h << 4 | l);
                        i += 4;
                    } else {
                        // Invalid hex, pass through as-is
                        out.push(b'\\');
                        i += 1;
                    }
                }
                _ => {
                    // Unrecognized escape, pass through
                    out.push(b'\\');
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    out
}

/// Convert an ASCII hex digit to its numeric value (0-15).
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
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
