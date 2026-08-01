use crate::model::{HandyCommand, Project, TargetKind, TargetRef};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::time::{timeout, Duration};

const LOG_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const LINE_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEntry {
    pub command_id: String,
    pub status: ProcessStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub command_id: String,
    pub stream: LogStream,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

struct LogBuffer {
    entries: VecDeque<LogEntry>,
    bytes: usize,
    next_sequence: u64,
}

struct ProcessControl {
    stop: mpsc::Sender<()>,
}

pub struct RuntimeManager {
    app: AppHandle,
    entries: Mutex<HashMap<String, RuntimeEntry>>,
    logs: Mutex<LogBuffer>,
    controls: AsyncMutex<HashMap<String, ProcessControl>>,
    active_targets: AsyncMutex<HashMap<TargetRef, HashSet<String>>>,
}

impl RuntimeManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            entries: Mutex::new(HashMap::new()),
            logs: Mutex::new(LogBuffer {
                entries: VecDeque::new(),
                bytes: 0,
                next_sequence: 1,
            }),
            controls: AsyncMutex::new(HashMap::new()),
            active_targets: AsyncMutex::new(HashMap::new()),
        }
    }

    pub fn snapshot(&self) -> Vec<RuntimeEntry> {
        self.entries.lock().unwrap().values().cloned().collect()
    }

    pub fn log_snapshot(&self) -> Vec<LogEntry> {
        self.logs.lock().unwrap().entries.iter().cloned().collect()
    }

    pub fn clear_logs(&self) {
        let mut buffer = self.logs.lock().unwrap();
        buffer.entries.clear();
        buffer.bytes = 0;
    }

    pub async fn activate(&self, target: TargetRef, commands: HashSet<String>) {
        self.active_targets.lock().await.insert(target, commands);
    }

    pub async fn deactivate(&self, target: &TargetRef, commands: HashSet<String>) -> Vec<String> {
        let mut active = self.active_targets.lock().await;
        active.remove(target);
        if target.kind == TargetKind::Command {
            return commands.into_iter().collect();
        }
        let still_needed: HashSet<_> = active.values().flatten().cloned().collect();
        commands
            .into_iter()
            .filter(|id| !still_needed.contains(id))
            .collect()
    }

    pub async fn start(
        self: &Arc<Self>,
        command: HandyCommand,
        project: Project,
    ) -> Result<(), String> {
        {
            let mut entries = self.entries.lock().unwrap();
            if matches!(
                entries.get(&command.id).map(|entry| entry.status),
                Some(ProcessStatus::Starting | ProcessStatus::Running | ProcessStatus::Stopping)
            ) {
                return Ok(());
            }
            entries.insert(
                command.id.clone(),
                RuntimeEntry {
                    command_id: command.id.clone(),
                    status: ProcessStatus::Starting,
                    exit_code: None,
                    started_at: Some(now()),
                },
            );
        }
        self.emit_runtime();

        let cwd = PathBuf::from(&project.base_dir).join(&command.cwd);
        let mut process = shell_command(&command.command, cwd);
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.set_status(&command.id, ProcessStatus::Failed, None);
                self.push_log(
                    &command.id,
                    LogStream::System,
                    format!("Could not start: {error}"),
                );
                return Err(error.to_string());
            }
        };
        let pid = child
            .id()
            .ok_or_else(|| "The process started without a process id".to_string())?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (stop, stop_receiver) = mpsc::channel(1);
        self.controls
            .lock()
            .await
            .insert(command.id.clone(), ProcessControl { stop });
        self.set_status(&command.id, ProcessStatus::Running, None);
        self.push_log(
            &command.id,
            LogStream::System,
            format!("Started in {}", project.base_dir),
        );

        if let Some(stdout) = stdout {
            tokio::spawn(read_output(
                Arc::clone(self),
                command.id.clone(),
                LogStream::Stdout,
                stdout,
            ));
        }
        if let Some(stderr) = stderr {
            tokio::spawn(read_output(
                Arc::clone(self),
                command.id.clone(),
                LogStream::Stderr,
                stderr,
            ));
        }

        let runtime = Arc::clone(self);
        let command_id = command.id;
        tokio::spawn(async move {
            let (was_stopped, result) = wait_or_stop(&mut child, pid, stop_receiver).await;
            runtime.controls.lock().await.remove(&command_id);
            match result {
                Ok(status) if was_stopped => {
                    runtime.set_status(&command_id, ProcessStatus::Stopped, status.code());
                    runtime.push_log(&command_id, LogStream::System, "Stopped".into());
                }
                Ok(status) if status.success() => {
                    runtime.set_status(&command_id, ProcessStatus::Completed, status.code());
                    runtime.push_log(&command_id, LogStream::System, "Completed".into());
                }
                Ok(status) => {
                    runtime.set_status(&command_id, ProcessStatus::Failed, status.code());
                    runtime.push_log(
                        &command_id,
                        LogStream::System,
                        format!(
                            "Exited with code {}",
                            status
                                .code()
                                .map_or_else(|| "unknown".into(), |code| code.to_string())
                        ),
                    );
                }
                Err(error) => {
                    runtime.set_status(&command_id, ProcessStatus::Failed, None);
                    runtime.push_log(
                        &command_id,
                        LogStream::System,
                        format!("Process error: {error}"),
                    );
                }
            }
        });
        Ok(())
    }

    pub async fn stop(&self, command: &HandyCommand, project: &Project) {
        let sender = self
            .controls
            .lock()
            .await
            .get(&command.id)
            .map(|control| control.stop.clone());
        let Some(sender) = sender else { return };
        self.set_status(&command.id, ProcessStatus::Stopping, None);
        if let Some(stop_command) = command
            .stop_command
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let mut process = shell_command(
                stop_command,
                PathBuf::from(&project.base_dir).join(&command.cwd),
            );
            if let Ok(mut child) = process.spawn() {
                let _ = timeout(Duration::from_secs(8), child.wait()).await;
            }
        }
        let _ = sender.send(()).await;
    }

    fn set_status(&self, command_id: &str, status: ProcessStatus, exit_code: Option<i32>) {
        let mut entries = self.entries.lock().unwrap();
        let started_at = entries.get(command_id).and_then(|entry| entry.started_at);
        entries.insert(
            command_id.into(),
            RuntimeEntry {
                command_id: command_id.into(),
                status,
                exit_code,
                started_at,
            },
        );
        drop(entries);
        self.emit_runtime();
    }

    fn emit_runtime(&self) {
        let _ = self.app.emit("runtime-changed", self.snapshot());
    }

    fn push_log(&self, command_id: &str, stream: LogStream, mut text: String) {
        if text.len() > LINE_LIMIT_BYTES {
            let mut boundary = LINE_LIMIT_BYTES;
            while !text.is_char_boundary(boundary) {
                boundary -= 1;
            }
            text.truncate(boundary);
            text.push_str(" … [line truncated]");
        }
        let mut buffer = self.logs.lock().unwrap();
        let entry = LogEntry {
            sequence: buffer.next_sequence,
            timestamp: now(),
            command_id: command_id.into(),
            stream,
            text,
        };
        buffer.next_sequence += 1;
        buffer.bytes += entry.text.len();
        buffer.entries.push_back(entry.clone());
        while buffer.bytes > LOG_LIMIT_BYTES {
            if let Some(removed) = buffer.entries.pop_front() {
                buffer.bytes = buffer.bytes.saturating_sub(removed.text.len());
            }
        }
        drop(buffer);
        let _ = self.app.emit("log-batch", vec![entry]);
    }
}

async fn read_output<R>(
    runtime: Arc<RuntimeManager>,
    command_id: String,
    stream: LogStream,
    output: R,
) where
    R: AsyncRead + Unpin,
{
    let mut lines = BufReader::new(output).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => runtime.push_log(&command_id, stream, line),
            Ok(None) => break,
            Err(error) => {
                runtime.push_log(
                    &command_id,
                    LogStream::System,
                    format!("Log read error: {error}"),
                );
                break;
            }
        }
    }
}

async fn wait_or_stop(
    child: &mut tokio::process::Child,
    pid: u32,
    mut stop: mpsc::Receiver<()>,
) -> (bool, std::io::Result<ExitStatus>) {
    tokio::select! {
        result = child.wait() => (false, result),
        _ = stop.recv() => {
            terminate_tree(pid, false).await;
            match timeout(Duration::from_secs(3), child.wait()).await {
                Ok(result) => (true, result),
                Err(_) => {
                    terminate_tree(pid, true).await;
                    (true, child.wait().await)
                }
            }
        }
    }
}

fn shell_command(script: &str, cwd: PathBuf) -> tokio::process::Command {
    #[cfg(unix)]
    let mut command = {
        use std::os::unix::process::CommandExt;
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
        let mut command = std::process::Command::new(shell);
        command.args(["-lc", script]).process_group(0);
        command
    };

    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args(["-NoLogo", "-NoProfile", "-Command", script])
            .creation_flags(0x0000_0200);
        command
    };

    command
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    tokio::process::Command::from(command)
}

#[cfg(unix)]
async fn terminate_tree(pid: u32, force: bool) {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

#[cfg(windows)]
async fn terminate_tree(pid: u32, _force: bool) {
    let _ = tokio::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
