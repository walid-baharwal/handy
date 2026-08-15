use crate::model::{Config, HandyCommand, Project, TargetKind, TargetRef};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout, Duration};

const LOG_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const LINE_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
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
    app: Option<AppHandle>,
    entries: Mutex<HashMap<String, RuntimeEntry>>,
    logs: Mutex<LogBuffer>,
    controls: AsyncMutex<HashMap<String, ProcessControl>>,
    active_targets: AsyncMutex<HashMap<TargetRef, HashSet<String>>>,
}

impl RuntimeManager {
    pub fn new(app: AppHandle) -> Self {
        Self::with_app(Some(app))
    }

    fn with_app(app: Option<AppHandle>) -> Self {
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

    pub fn can_stop(&self, command: &HandyCommand) -> bool {
        let status = self
            .entries
            .lock()
            .unwrap()
            .get(&command.id)
            .map(|entry| entry.status);
        matches!(
            status,
            Some(ProcessStatus::Starting | ProcessStatus::Running | ProcessStatus::Stopping)
        ) || (status == Some(ProcessStatus::Completed)
            && command
                .stop_command
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()))
    }

    pub async fn shutdown(self: &Arc<Self>, config: Config) {
        let mut stops = JoinSet::new();
        for command in config
            .commands
            .values()
            .filter(|command| self.can_stop(command))
        {
            let Some(project) = config.projects.get(&command.project_id) else {
                continue;
            };
            let runtime = Arc::clone(self);
            let command = command.clone();
            let project = project.clone();
            stops.spawn(async move { runtime.stop(&command, &project).await });
        }
        while stops.join_next().await.is_some() {}
        self.active_targets.lock().await.clear();
        let _ = timeout(Duration::from_secs(4), async {
            while !self.controls.lock().await.is_empty() {
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
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
        let stop_command = command
            .stop_command
            .as_deref()
            .filter(|value| !value.trim().is_empty());

        if sender.is_none() && stop_command.is_none() {
            return;
        }
        if sender.is_some() {
            self.set_status(&command.id, ProcessStatus::Stopping, None);
        }
        if let Some(stop_command) = stop_command {
            self.push_log(
                &command.id,
                LogStream::System,
                format!("Running stop command: {stop_command}"),
            );
            let mut process = shell_command(
                stop_command,
                PathBuf::from(&project.base_dir).join(&command.cwd),
            );
            match timeout(Duration::from_secs(8), process.output()).await {
                Ok(Ok(output)) => {
                    self.push_cleanup_output(&command.id, LogStream::Stdout, output.stdout);
                    self.push_cleanup_output(&command.id, LogStream::Stderr, output.stderr);
                    if !output.status.success() {
                        self.push_log(
                            &command.id,
                            LogStream::System,
                            "Configured stop command failed".into(),
                        );
                    }
                }
                Ok(Err(_)) => self.push_log(
                    &command.id,
                    LogStream::System,
                    "Could not start the configured stop command".into(),
                ),
                Err(_) => self.push_log(
                    &command.id,
                    LogStream::System,
                    "Configured stop command timed out".into(),
                ),
            }
        }
        if let Some(sender) = sender {
            let _ = sender.send(()).await;
        } else {
            self.set_status(&command.id, ProcessStatus::Stopped, None);
            self.push_log(
                &command.id,
                LogStream::System,
                "Configured cleanup completed".into(),
            );
        }
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
        if let Some(app) = &self.app {
            let _ = app.emit("runtime-changed", self.snapshot());
        }
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
        if let Some(app) = &self.app {
            let _ = app.emit("log-batch", vec![entry]);
        }
    }

    fn push_cleanup_output(&self, command_id: &str, stream: LogStream, output: Vec<u8>) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines().filter(|line| !line.is_empty()) {
            self.push_log(command_id, stream, line.into());
        }
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
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    command
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

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project {
            id: "project".into(),
            name: "Test project".into(),
            base_dir: std::env::current_dir().unwrap().display().to_string(),
            command_ids: vec![],
        }
    }

    fn command(id: &str, command: &str, stop_command: Option<&str>) -> HandyCommand {
        HandyCommand {
            id: id.into(),
            project_id: "project".into(),
            name: id.into(),
            command: command.into(),
            cwd: ".".into(),
            stop_command: stop_command.map(str::to_owned),
        }
    }

    async fn wait_for_status(runtime: &RuntimeManager, id: &str, status: ProcessStatus) {
        for _ in 0..100 {
            if runtime
                .snapshot()
                .iter()
                .any(|entry| entry.command_id == id && entry.status == status)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("{id} did not reach {status:?}");
    }

    async fn wait_for_log(runtime: &RuntimeManager, text: &str) {
        for _ in 0..100 {
            if runtime
                .log_snapshot()
                .iter()
                .any(|entry| entry.text == text)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("log entry {text:?} was not captured");
    }

    #[tokio::test]
    async fn starts_stops_and_captures_both_log_streams() {
        let runtime = Arc::new(RuntimeManager::with_app(None));
        let project = project();
        let command = command(
            "server",
            "printf 'stdout\\n'; printf 'stderr\\n' >&2; sleep 30",
            None,
        );

        runtime
            .start(command.clone(), project.clone())
            .await
            .unwrap();
        wait_for_status(&runtime, "server", ProcessStatus::Running).await;
        wait_for_log(&runtime, "stdout").await;
        wait_for_log(&runtime, "stderr").await;

        runtime.stop(&command, &project).await;
        wait_for_status(&runtime, "server", ProcessStatus::Stopped).await;
    }

    #[tokio::test]
    async fn completed_command_runs_its_configured_cleanup() {
        let runtime = Arc::new(RuntimeManager::with_app(None));
        let project = project();
        let command = command("database", "true", Some("printf 'cleanup\\n'"));

        runtime
            .start(command.clone(), project.clone())
            .await
            .unwrap();
        wait_for_status(&runtime, "database", ProcessStatus::Completed).await;

        runtime.stop(&command, &project).await;
        wait_for_status(&runtime, "database", ProcessStatus::Stopped).await;
        wait_for_log(&runtime, "cleanup").await;
        wait_for_log(&runtime, "Configured cleanup completed").await;
    }

    #[tokio::test]
    async fn recipe_start_and_stop_preserves_shared_command_ownership() {
        let runtime = Arc::new(RuntimeManager::with_app(None));
        let project = project();
        let command = command("api", "sleep 30", None);
        let first = TargetRef {
            kind: TargetKind::Group,
            id: "first".into(),
        };
        let second = TargetRef {
            kind: TargetKind::Group,
            id: "second".into(),
        };
        let commands = HashSet::from(["api".into()]);

        runtime.activate(first.clone(), commands.clone()).await;
        runtime
            .start(command.clone(), project.clone())
            .await
            .unwrap();
        wait_for_status(&runtime, "api", ProcessStatus::Running).await;
        runtime.activate(second.clone(), commands.clone()).await;

        assert!(runtime
            .deactivate(&first, commands.clone())
            .await
            .is_empty());
        assert_eq!(runtime.deactivate(&second, commands).await, vec!["api"]);
        runtime.stop(&command, &project).await;
        wait_for_status(&runtime, "api", ProcessStatus::Stopped).await;
    }

    #[tokio::test]
    async fn shutdown_stops_every_running_command() {
        let runtime = Arc::new(RuntimeManager::with_app(None));
        let mut project = project();
        let first = command("api", "sleep 30", None);
        let second = command("worker", "sleep 30", None);
        project.command_ids = vec![first.id.clone(), second.id.clone()];

        runtime.start(first.clone(), project.clone()).await.unwrap();
        runtime
            .start(second.clone(), project.clone())
            .await
            .unwrap();
        wait_for_status(&runtime, "api", ProcessStatus::Running).await;
        wait_for_status(&runtime, "worker", ProcessStatus::Running).await;

        let config = Config {
            schema_version: 1,
            projects: HashMap::from([(project.id.clone(), project)]),
            commands: HashMap::from([(first.id.clone(), first), (second.id.clone(), second)]),
            groups: HashMap::new(),
        };
        runtime.shutdown(config).await;

        wait_for_status(&runtime, "api", ProcessStatus::Stopped).await;
        wait_for_status(&runtime, "worker", ProcessStatus::Stopped).await;
    }
}
