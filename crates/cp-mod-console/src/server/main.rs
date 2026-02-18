//! Console Server: persistent daemon that owns child processes.
//!
//! Spawns `script -q -f -c` processes and holds their stdin pipes.
//! TUI communicates via JSON lines over a Unix socket.
//! Survives TUI exit/reload — processes stay alive.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Request {
    cmd: String,
    key: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    input: Option<String>,
    log_path: Option<String>,
}

#[derive(Serialize)]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions: Option<Vec<SessionInfo>>,
}

#[derive(Serialize)]
struct SessionInfo {
    key: String,
    pid: u32,
    status: String,
    exit_code: Option<i32>,
}

impl Response {
    fn ok() -> Self {
        Self { ok: true, error: None, pid: None, status: None, exit_code: None, sessions: None }
    }
    fn ok_pid(pid: u32) -> Self {
        Self { ok: true, error: None, pid: Some(pid), status: None, exit_code: None, sessions: None }
    }
    fn ok_status(status: String, exit_code: Option<i32>) -> Self {
        Self { ok: true, error: None, pid: None, status: Some(status), exit_code, sessions: None }
    }
    fn ok_sessions(sessions: Vec<SessionInfo>) -> Self {
        Self { ok: true, error: None, pid: None, status: None, exit_code: None, sessions: Some(sessions) }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, error: Some(msg.into()), pid: None, status: None, exit_code: None, sessions: None }
    }
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

struct Session {
    pid: u32,
    stdin: Option<std::process::ChildStdin>,
    status: SessionStatus,
}

#[derive(Clone)]
enum SessionStatus {
    Running,
    Exited(i32),
}

impl Session {
    /// Check if the process has exited (non-blocking).
    fn poll_status(&mut self) {
        if matches!(self.status, SessionStatus::Running) {
            if !is_pid_alive(self.pid) {
                // Try to get exit code from /proc/{pid}/status or fall back to -1
                self.status = SessionStatus::Exited(-1);
            }
        }
    }

    fn status_str(&self) -> String {
        match &self.status {
            SessionStatus::Running => "running".to_string(),
            SessionStatus::Exited(code) => format!("exited({})", code),
        }
    }

    fn exit_code(&self) -> Option<i32> {
        match &self.status {
            SessionStatus::Running => None,
            SessionStatus::Exited(c) => Some(*c),
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(self.status, SessionStatus::Exited(_))
    }
}

fn is_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

type Sessions = Arc<Mutex<HashMap<String, Session>>>;

// ---------------------------------------------------------------------------
// Command handlers
// ---------------------------------------------------------------------------

fn handle_create(sessions: &Sessions, key: &str, command: &str, cwd: Option<&str>, log_path: &str) -> Response {
    let log = PathBuf::from(log_path);

    // Create/truncate log file
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&log, b"") {
        return Response::err(format!("Failed to create log: {}", e));
    }

    let mut cmd = Command::new("script");
    cmd.args(["-q", "-f", "-c", command, log_path]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    // Detach into own process group
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Response::err(format!("Spawn failed: {}", e)),
    };

    let pid = child.id();
    let stdin = child.stdin.take();

    // Spawn a thread to wait for the child so we get proper exit status
    {
        let sessions = Arc::clone(sessions);
        let key = key.to_string();
        std::thread::spawn(move || {
            let code = match child.wait() {
                Ok(status) => status.code().unwrap_or(-1),
                Err(_) => -1,
            };
            if let Ok(mut map) = sessions.lock() {
                if let Some(session) = map.get_mut(&key) {
                    session.status = SessionStatus::Exited(code);
                }
            }
        });
    }

    let session = Session { pid, stdin, status: SessionStatus::Running };
    sessions.lock().unwrap().insert(key.to_string(), session);

    Response::ok_pid(pid)
}

fn handle_send(sessions: &Sessions, key: &str, input: &str) -> Response {
    let bytes = interpret_escapes(input);
    let mut map = sessions.lock().unwrap();
    let session = match map.get_mut(key) {
        Some(s) => s,
        None => return Response::err(format!("Session '{}' not found", key)),
    };
    if session.is_terminal() {
        return Response::err(format!("Session '{}' already exited", key));
    }
    match &mut session.stdin {
        Some(stdin) => {
            if let Err(e) = stdin.write_all(&bytes) {
                return Response::err(format!("Write failed: {}", e));
            }
            if let Err(e) = stdin.flush() {
                return Response::err(format!("Flush failed: {}", e));
            }
            Response::ok()
        }
        None => Response::err("No stdin available".to_string()),
    }
}

fn handle_kill(sessions: &Sessions, key: &str) -> Response {
    let mut map = sessions.lock().unwrap();
    let session = match map.get_mut(key) {
        Some(s) => s,
        None => return Response::err(format!("Session '{}' not found", key)),
    };
    if !session.is_terminal() {
        // SIGTERM to process group (script + all children)
        // Safe because process_group(0) at spawn ensures PGID = script PID
        let _ = Command::new("kill").args([&format!("-{}", session.pid)]).output();
        std::thread::sleep(std::time::Duration::from_millis(100));
        if is_pid_alive(session.pid) {
            let _ = Command::new("kill").args(["-9", &format!("-{}", session.pid)]).output();
        }
        session.status = SessionStatus::Exited(-9);
    }
    // Drop stdin
    session.stdin.take();
    Response::ok()
}

fn handle_remove(sessions: &Sessions, key: &str) -> Response {
    let mut map = sessions.lock().unwrap();
    if let Some(mut session) = map.remove(key) {
        if !session.is_terminal() {
            let _ = Command::new("kill").args([&format!("-{}", session.pid)]).output();
            std::thread::sleep(std::time::Duration::from_millis(100));
            if is_pid_alive(session.pid) {
                let _ = Command::new("kill").args(["-9", &format!("-{}", session.pid)]).output();
            }
        }
        session.stdin.take();
    }
    Response::ok()
}

fn handle_status(sessions: &Sessions, key: &str) -> Response {
    let mut map = sessions.lock().unwrap();
    let session = match map.get_mut(key) {
        Some(s) => s,
        None => return Response::err(format!("Session '{}' not found", key)),
    };
    session.poll_status();
    Response::ok_status(session.status_str(), session.exit_code())
}

fn handle_list(sessions: &Sessions) -> Response {
    let mut map = sessions.lock().unwrap();
    let infos: Vec<SessionInfo> = map
        .iter_mut()
        .map(|(key, session)| {
            session.poll_status();
            SessionInfo {
                key: key.clone(),
                pid: session.pid,
                status: session.status_str(),
                exit_code: session.exit_code(),
            }
        })
        .collect();
    Response::ok_sessions(infos)
}

// ---------------------------------------------------------------------------
// Escape sequence interpreter (same as manager.rs)
// ---------------------------------------------------------------------------

fn interpret_escapes(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'n' => { out.push(0x0A); i += 2; }
                b'r' => { out.push(0x0D); i += 2; }
                b't' => { out.push(0x09); i += 2; }
                b'\\' => { out.push(b'\\'); i += 2; }
                b'e' => { out.push(0x1B); i += 2; }
                b'0' => { out.push(0x00); i += 2; }
                b'x' if i + 3 < bytes.len() => {
                    let hi = bytes[i + 2];
                    let lo = bytes[i + 3];
                    if let (Some(h), Some(l)) = (hex_digit(hi), hex_digit(lo)) {
                        out.push(h << 4 | l);
                        i += 4;
                    } else {
                        out.push(b'\\');
                        i += 1;
                    }
                }
                _ => { out.push(b'\\'); i += 1; }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

fn handle_connection(stream: UnixStream, sessions: Sessions) {
    let reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break, // Connection closed
        };
        if line.is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(format!("Invalid JSON: {}", e));
                let _ = writeln!(writer, "{}", serde_json::to_string(&resp).unwrap());
                continue;
            }
        };

        let resp = match req.cmd.as_str() {
            "create" => {
                let key = req.key.as_deref().unwrap_or("");
                let command = req.command.as_deref().unwrap_or("");
                let log_path = req.log_path.as_deref().unwrap_or("");
                if key.is_empty() || command.is_empty() || log_path.is_empty() {
                    Response::err("Missing key, command, or log_path")
                } else {
                    handle_create(&sessions, key, command, req.cwd.as_deref(), log_path)
                }
            }
            "send" => {
                let key = req.key.as_deref().unwrap_or("");
                let input = req.input.as_deref().unwrap_or("");
                if key.is_empty() {
                    Response::err("Missing key")
                } else {
                    handle_send(&sessions, key, input)
                }
            }
            "kill" => {
                let key = req.key.as_deref().unwrap_or("");
                if key.is_empty() { Response::err("Missing key") } else { handle_kill(&sessions, key) }
            }
            "remove" => {
                let key = req.key.as_deref().unwrap_or("");
                if key.is_empty() { Response::err("Missing key") } else { handle_remove(&sessions, key) }
            }
            "status" => {
                let key = req.key.as_deref().unwrap_or("");
                if key.is_empty() { Response::err("Missing key") } else { handle_status(&sessions, key) }
            }
            "list" => handle_list(&sessions),
            "ping" => Response::ok(),
            "shutdown" => {
                // Kill all sessions and exit
                let mut map = sessions.lock().unwrap();
                for (_, session) in map.iter_mut() {
                    if !session.is_terminal() {
                        let _ = Command::new("kill").args([&session.pid.to_string()]).output();
                    }
                    session.stdin.take();
                }
                map.clear();
                let resp = Response::ok();
                let _ = writeln!(writer, "{}", serde_json::to_string(&resp).unwrap());
                std::process::exit(0);
            }
            other => Response::err(format!("Unknown command: {}", other)),
        };

        if writeln!(writer, "{}", serde_json::to_string(&resp).unwrap()).is_err() {
            break; // Connection lost
        }
    }
}

// ---------------------------------------------------------------------------
// Main: daemonize and listen
// ---------------------------------------------------------------------------

fn main() {
    let socket_path = std::env::args().nth(1).expect("Usage: cp-console-server <socket_path>");
    let pid_path = format!("{}.pid", socket_path.trim_end_matches(".sock"));

    // Remove stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Daemonize: new session, close stdio
    unsafe {
        libc_setsid();
    }

    // Write PID file
    let _ = std::fs::write(&pid_path, format!("{}", std::process::id()));

    // Bind socket
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind {}: {}", socket_path, e);
            std::process::exit(1);
        }
    };

    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    // Accept connections (one thread per connection)
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let sessions = Arc::clone(&sessions);
                std::thread::spawn(move || {
                    handle_connection(stream, sessions);
                });
            }
            Err(_) => continue,
        }
    }
}

/// Minimal setsid() without libc crate dependency.
unsafe fn libc_setsid() {
    // setsid() syscall number on x86_64 Linux = 112
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::asm!(
            "syscall",
            in("rax") 112u64,
            out("rcx") _,
            out("r11") _,
        );
    }
    // On other architectures, skip silently
}
