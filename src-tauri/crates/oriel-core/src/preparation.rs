//! Per-page preparation commands. Tool business state stays with the called tool.
use crate::{normalize_url, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    #[default]
    Auto,
    Direct,
    Cmd,
    Powershell,
    Custom,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Once,
    Persistent,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Interpreter {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preparation {
    pub path: PathBuf,
    #[serde(default)]
    pub runner: Runner,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpreter: Option<Interpreter>,
}
fn default_timeout() -> u64 {
    120
}
fn device_absolute(path: &Path) -> bool {
    let s = path.to_string_lossy();
    path.is_absolute()
        || (s.len() >= 3
            && s.as_bytes()[0].is_ascii_alphabetic()
            && s.as_bytes()[1] == b':'
            && matches!(s.as_bytes()[2], b'\\' | b'/'))
        || s.starts_with("\\\\")
}
fn safe_text(s: &str) -> bool {
    !s.contains(['\0', '\r', '\n'])
}
// PowerShell and CMD accept ordinary drive/UNC paths; canonicalization on
// Windows uses extended-length prefixes that these interpreters may not accept.
fn execution_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(drive) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(drive);
        }
    }
    path
}
impl Preparation {
    pub fn validate(&self) -> Result<()> {
        if self.path.as_os_str().is_empty()
            || !safe_text(&self.path.to_string_lossy())
            || !(1..=3600).contains(&self.timeout_seconds)
            || self.args.iter().any(|s| !safe_text(s))
            || self
                .cwd
                .as_ref()
                .is_some_and(|p| p.as_os_str().is_empty() || !safe_text(&p.to_string_lossy()))
        {
            return Err("前置程序须提供有效路径、参数及 1–3600 秒的准备超时。".into());
        }
        if self.runner == Runner::Custom {
            let i = self.interpreter.as_ref().ok_or("请选择自定义解释器。")?;
            if i.program.trim().is_empty()
                || !safe_text(&i.program)
                || i.args.iter().any(|s| !safe_text(s))
            {
                return Err("自定义解释器或参数无效。".into());
            }
        } else if self.interpreter.is_some() {
            return Err("只有自定义执行方式可以指定解释器。".into());
        }
        self.effective_runner()?;
        Ok(())
    }
    pub fn validate_local(&self) -> Result<()> {
        self.validate()?;
        if !device_absolute(&self.path) || self.cwd.as_ref().is_some_and(|p| !device_absolute(p)) {
            return Err("本地前置程序和工作目录须使用当前设备的绝对路径。".into());
        }
        Ok(())
    }
    fn effective_runner(&self) -> Result<Runner> {
        if self.runner != Runner::Auto {
            return Ok(self.runner.clone());
        }
        match self
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "exe" => Ok(Runner::Direct),
            "cmd" | "bat" => Ok(Runner::Cmd),
            "ps1" => Ok(Runner::Powershell),
            _ => Err("无法自动选择执行方式，请指定解释器或直接执行。".into()),
        }
    }
    pub fn resolve(&self, directory: Option<&Path>) -> Result<Self> {
        self.validate()?;
        if directory.is_none() {
            self.validate_local()?;
        }
        let base = directory
            .map(|p| p.canonicalize().map_err(|e| e.to_string()))
            .transpose()?;
        let resolve = |p: &Path| -> Result<PathBuf> {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                base.as_ref().ok_or("前置程序需要绝对路径。")?.join(p)
            };
            let path = path
                .canonicalize()
                .map_err(|e| format!("前置程序路径不可用：{e}"))?;
            if base.as_ref().is_some_and(|base| !path.starts_with(base)) {
                return Err("插件前置程序和工作目录须位于插件目录内。".into());
            }
            Ok(execution_path(path))
        };
        let mut resolved = self.clone();
        resolved.path = resolve(&self.path)?;
        if !resolved.path.is_file() {
            return Err("前置程序不是普通文件。".into());
        }
        resolved.cwd = Some(match &self.cwd {
            Some(p) => resolve(p)?,
            None => resolved
                .path
                .parent()
                .ok_or("程序缺少父目录。")?
                .to_path_buf(),
        });
        if !resolved.cwd.as_ref().unwrap().is_dir() {
            return Err("前置程序工作目录无效。".into());
        }
        Ok(resolved)
    }
    pub fn command(&self) -> Result<Command> {
        let path = self.path.to_string_lossy();
        let mut cmd = match self.effective_runner()? {
            Runner::Direct => Command::new(&self.path),
            Runner::Powershell => {
                let mut cmd = Command::new("powershell.exe");
                cmd.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(&self.path);
                cmd
            }
            Runner::Cmd => {
                // Values are expanded once through private environment variables rather than
                // interpolated into shell syntax. Delayed expansion stays disabled.
                let mut cmd = Command::new("cmd.exe");
                let mut invocation = String::from("\"\"%ORIEL_PREP_PATH%\"");
                cmd.env("ORIEL_PREP_PATH", path.as_ref());
                for (index, value) in self.args.iter().enumerate() {
                    // Quotes and line breaks cannot be transported as literal batch arguments.
                    if value.contains('"') {
                        return Err("CMD 参数不支持双引号，请使用 PowerShell 或包装脚本。".into());
                    }
                    let key = format!("ORIEL_PREP_ARG_{index}");
                    cmd.env(&key, value);
                    invocation.push_str(&format!(" \"%{key}%\""));
                }
                invocation.push('"');
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.args(["/D", "/V:OFF", "/S", "/C"]).raw_arg(invocation);
                }
                #[cfg(not(windows))]
                cmd.args(["/D", "/V:OFF", "/S", "/C"]).arg(invocation);
                return Ok(self.finish_command(cmd));
            }
            Runner::Custom => {
                let i = self.interpreter.as_ref().ok_or("缺少解释器。")?;
                let mut cmd = Command::new(&i.program);
                cmd.args(&i.args).arg(&self.path);
                cmd
            }
            Runner::Auto => unreachable!(),
        };
        cmd.args(&self.args);
        Ok(self.finish_command(cmd))
    }
    fn finish_command(&self, mut cmd: Command) -> Command {
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparationResult {
    #[serde(rename = "type")]
    pub kind: String,
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}
pub fn parse_result(line: &[u8]) -> Result<Option<String>> {
    let value: PreparationResult = serde_json::from_slice(line)
        .map_err(|e| format!("前置程序结果不是有效的 UTF-8 JSON：{e}"))?;
    if value.kind != "result" {
        return Err("前置程序只接受 type=result 的准备结果。".into());
    }
    if !value.success {
        return Err(value
            .error
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "前置程序报告失败。".into()));
    }
    if value.error.is_some() {
        return Err("成功结果不能同时包含失败原因。".into());
    }
    value.url.map(|url| normalize_url(&url)).transpose()
}

/// Platform ownership must cover the whole process tree, including root exit.
pub trait ProcessTree: Send {
    fn stop(&mut self, child: &mut Child) -> Result<()>;
}
pub struct OwnedProcess {
    pub child: Child,
    pub tree: Box<dyn ProcessTree>,
}
pub type Launcher = fn(&mut Command) -> Result<OwnedProcess>;
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    Starting,
    Ready,
    Completed,
    Failed,
    Stopping,
    Stopped,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    pub entry_id: String,
    pub title: String,
    pub generation: u64,
    pub state: State,
    pub url: Option<String>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
    pub cleaned: bool,
    pub logs: VecDeque<String>,
}
#[derive(Clone, Debug)]
pub struct PreparedOutcome {
    pub generation: u64,
    pub url: Option<String>,
}
struct Run {
    spec: Preparation,
    view: Mutex<RunView>,
    changed: Condvar,
    cancel: AtomicBool,
    rejection: Mutex<Option<String>>,
}
impl Run {
    fn wait(&self) -> Result<PreparedOutcome> {
        let mut v = self.view.lock().unwrap();
        loop {
            match v.state {
                State::Ready | State::Completed => {
                    return Ok(PreparedOutcome {
                        generation: v.generation,
                        url: v.url.clone(),
                    })
                }
                State::Failed | State::Stopped => {
                    return Err(v.error.clone().unwrap_or_else(|| "前置程序已停止。".into()))
                }
                _ => v = self.changed.wait(v).unwrap(),
            }
        }
    }
    fn stop(&self) -> Result<()> {
        let mut view = self.view.lock().unwrap();
        if view.cleaned {
            return Ok(());
        }
        self.cancel.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !view.cleaned {
            if Instant::now() >= deadline {
                return Err("无法确认前置程序已清理，禁止重叠启动。".into());
            }
            view = self
                .changed
                .wait_timeout(view, Duration::from_millis(25))
                .unwrap()
                .0;
        }
        Ok(())
    }
    fn log(&self, text: String) {
        let mut view = self.view.lock().unwrap();
        let mut end = text.len().min(4096);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        view.logs.push_back(text[..end].to_owned());
        while view.logs.len() > 256 {
            view.logs.pop_front();
        }
    }
}
#[derive(Default)]
struct Slot {
    gate: Mutex<()>,
    run: Mutex<Option<Arc<Run>>>,
    blocked: AtomicBool,
}
#[derive(Default)]
struct Registry {
    slots: HashMap<String, Arc<Slot>>,
    closing: bool,
}
#[derive(Default)]
pub struct Preparations {
    registry: Mutex<Registry>,
    next: AtomicU64,
}
impl Preparations {
    fn slot(&self, id: &str) -> Arc<Slot> {
        self.registry
            .lock()
            .unwrap()
            .slots
            .entry(id.into())
            .or_default()
            .clone()
    }
    pub fn prepare(
        &self,
        id: &str,
        title: &str,
        spec: Preparation,
        retry: bool,
        launcher: Launcher,
    ) -> Result<PreparedOutcome> {
        let slot = self.slot(id);
        let run = {
            let _gate = slot.gate.lock().unwrap();
            if slot.blocked.load(Ordering::SeqCst) {
                return Err("网页入口正在移除或已移除。".into());
            }
            let existing = slot.run.lock().unwrap().clone();
            let mut reuse = None;
            if let Some(old) = existing {
                let state = old.view.lock().unwrap().state.clone();
                if old.spec == spec && (!retry || state == State::Starting) {
                    reuse = Some(old);
                } else if !retry {
                    return Err("前置程序配置已变更，请重新准备并加载。".into());
                } else {
                    old.stop()?;
                }
            }
            if let Some(old) = reuse {
                old
            } else {
                let registry = self.registry.lock().unwrap();
                if registry.closing {
                    return Err("Oriel 正在退出，不能启动前置程序。".into());
                }
                let run = Arc::new(Run {
                    spec,
                    view: Mutex::new(RunView {
                        entry_id: id.into(),
                        title: title.into(),
                        generation: self.next.fetch_add(1, Ordering::Relaxed) + 1,
                        state: State::Starting,
                        url: None,
                        error: None,
                        exit_code: None,
                        cleaned: false,
                        logs: VecDeque::new(),
                    }),
                    changed: Condvar::new(),
                    cancel: AtomicBool::new(false),
                    rejection: Mutex::new(None),
                });
                *slot.run.lock().unwrap() = Some(run.clone());
                let task = run.clone();
                thread::spawn(move || execute(task, launcher));
                drop(registry);
                run
            }
        };
        run.wait()
    }
    pub fn reject_result(&self, id: &str, generation: u64, error: &str) {
        let slot = self.slot(id);
        let _gate = slot.gate.lock().unwrap();
        let run = slot.run.lock().unwrap().clone();
        if let Some(run) = run {
            let mut view = run.view.lock().unwrap();
            if view.generation != generation {
                return;
            }
            *run.rejection.lock().unwrap() = Some(error.into());
            if view.cleaned {
                view.state = State::Failed;
                view.error = Some(error.into());
            } else {
                run.cancel.store(true, Ordering::SeqCst);
            }
            run.changed.notify_all();
        }
    }
    pub fn is_closing(&self) -> bool {
        self.registry.lock().unwrap().closing
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        let slot = self.slot(id);
        let _gate = slot.gate.lock().unwrap();
        let run = slot.run.lock().unwrap().clone();
        if let Some(run) = run {
            run.stop()?;
        }
        Ok(())
    }
    pub fn retire(&self, id: &str) -> Result<()> {
        let slot = self.slot(id);
        let _gate = slot.gate.lock().unwrap();
        slot.blocked.store(true, Ordering::SeqCst);
        let run = slot.run.lock().unwrap().clone();
        if let Some(run) = run {
            if let Err(error) = run.stop() {
                slot.blocked.store(false, Ordering::SeqCst);
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn allow(&self, id: &str) {
        self.slot(id).blocked.store(false, Ordering::SeqCst);
    }
    pub fn views(&self) -> Vec<RunView> {
        let slots: Vec<_> = self
            .registry
            .lock()
            .unwrap()
            .slots
            .values()
            .cloned()
            .collect();
        let mut views = slots
            .iter()
            .filter_map(|slot| {
                slot.run
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|r| r.view.lock().unwrap().clone())
            })
            .collect::<Vec<_>>();
        views.sort_by(|a, b| a.entry_id.cmp(&b.entry_id));
        views
    }
    pub fn matches(&self, id: &str, spec: &Preparation) -> bool {
        let slot = self.slot(id);
        let run = slot.run.lock().unwrap().clone();
        run.is_some_and(|r| r.spec == *spec)
    }
    pub fn begin_shutdown(&self) {
        self.registry.lock().unwrap().closing = true;
    }
    pub fn shutdown(&self) -> Vec<String> {
        self.begin_shutdown();
        let ids = self
            .registry
            .lock()
            .unwrap()
            .slots
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        ids.iter().filter_map(|id| self.stop(id).err()).collect()
    }
}
enum Output {
    Line(Vec<u8>),
    End,
    Error(String),
}
fn read_results(output: impl Read + Send + 'static, sender: mpsc::SyncSender<Output>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        loop {
            let mut line = Vec::new();
            match reader.by_ref().take(65537).read_until(b'\n', &mut line) {
                Ok(0) => {
                    let _ = sender.send(Output::End);
                    break;
                }
                Ok(_) if line.len() <= 65536 && line.ends_with(b"\n") => {
                    if sender.send(Output::Line(line)).is_err() {
                        break;
                    }
                }
                _ => {
                    let _ = sender.send(Output::Error(
                        "结果须为不超过 64 KiB、以换行结束的 UTF-8 JSON。".into(),
                    ));
                    break;
                }
            }
        }
    });
}
fn execute(run: Arc<Run>, launcher: Launcher) {
    if run.cancel.load(Ordering::SeqCst) {
        let mut view = run.view.lock().unwrap();
        view.cleaned = true;
        view.state = State::Stopped;
        view.error = Some("前置程序已停止。".into());
        run.changed.notify_all();
        return;
    }
    let spawn = run
        .spec
        .command()
        .and_then(|mut command| launcher(&mut command));
    let mut process = match spawn {
        Ok(p) => p,
        Err(error) => {
            let mut v = run.view.lock().unwrap();
            v.state = State::Failed;
            v.error = Some(error);
            v.cleaned = true;
            run.changed.notify_all();
            return;
        }
    };
    let (sender, receiver) = mpsc::sync_channel(8);
    read_results(
        process
            .child
            .stdout
            .take()
            .expect("launcher must preserve stdout"),
        sender,
    );
    let log_run = run.clone();
    let stderr = process
        .child
        .stderr
        .take()
        .expect("launcher must preserve stderr");
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        loop {
            let mut bytes = Vec::new();
            match reader.by_ref().take(4096).read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                _ => log_run.log(String::from_utf8_lossy(&bytes).trim_end().to_owned()),
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(run.spec.timeout_seconds);
    let mut result = None;
    let mut ended = false;
    let mut failure = None;
    let mut completed = false;
    loop {
        if run.cancel.load(Ordering::SeqCst) {
            break;
        }
        // Bounded channel and bounded batch prevent stdout from starving cancellation.
        for _ in 0..8 {
            match receiver.try_recv() {
                Ok(Output::Line(line)) => {
                    if result.is_some() {
                        failure = Some("前置程序重复返回准备结果。".into());
                        break;
                    }
                    match parse_result(&line) {
                        Ok(url) => result = Some(url),
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                Ok(Output::End) => ended = true,
                Ok(Output::Error(e)) => {
                    failure = Some(e);
                    break;
                }
                Err(_) => break,
            }
        }
        if failure.is_some() {
            break;
        }
        match process.child.try_wait() {
            Ok(Some(status)) => {
                run.view.lock().unwrap().exit_code = status.code();
                if run.spec.mode == Mode::Persistent {
                    failure = Some(format!("持续程序已退出（{status}）。"));
                    break;
                }
                if !status.success() {
                    failure = Some(format!("前置程序异常退出（{status}）。"));
                    break;
                }
                if ended {
                    if result.is_some() {
                        completed = true;
                    } else {
                        failure = Some("前置程序退出但未返回准备结果。".into());
                    }
                    break;
                }
            }
            Err(e) => {
                failure = Some(format!("读取程序状态失败：{e}"));
                break;
            }
            Ok(None) => (),
        }
        if result.is_some() && run.spec.mode == Mode::Persistent {
            let mut v = run.view.lock().unwrap();
            if v.state == State::Starting {
                v.url = result.clone().flatten();
                v.state = State::Ready;
                run.changed.notify_all();
            }
        }
        if Instant::now() >= deadline && (result.is_none() || run.spec.mode == Mode::Once) {
            failure = Some("前置程序准备超时。".into());
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    {
        let mut v = run.view.lock().unwrap();
        v.state = State::Stopping;
        run.changed.notify_all();
    }
    let cleanup = process.tree.stop(&mut process.child);
    failure = run.rejection.lock().unwrap().take().or(failure);
    let mut v = run.view.lock().unwrap();
    v.cleaned = cleanup.is_ok();
    v.exit_code = v.exit_code.or_else(|| {
        process
            .child
            .try_wait()
            .ok()
            .flatten()
            .and_then(|s| s.code())
    });
    if let Err(e) = cleanup {
        failure = Some(format!("{}；清理失败：{e}", failure.unwrap_or_default()));
    }
    if let Some(error) = failure {
        v.state = State::Failed;
        v.error = Some(error);
    } else if completed {
        v.state = State::Completed;
        v.url = result.flatten();
    } else {
        v.state = State::Stopped;
        v.error = Some("前置程序已停止。".into());
    }
    run.changed.notify_all();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::process::CommandExt, sync::atomic::AtomicUsize};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "oriel-preparation-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Group(u32);
    impl ProcessTree for Group {
        fn stop(&mut self, child: &mut Child) -> Result<()> {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", "--", &format!("-{}", self.0)])
                .stderr(Stdio::null())
                .status();
            child.wait().map(|_| ()).map_err(|e| e.to_string())
        }
    }
    fn launch(command: &mut Command) -> Result<OwnedProcess> {
        let child = command
            .process_group(0)
            .spawn()
            .map_err(|e| e.to_string())?;
        let tree = Box::new(Group(child.id()));
        Ok(OwnedProcess { child, tree })
    }
    fn fixture(scenario: &str, marker: &Path) -> Preparation {
        Preparation {
            path: Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/preparation.mjs"),
            runner: Runner::Custom,
            args: vec![scenario.into(), marker.to_string_lossy().into_owned()],
            cwd: None,
            mode: Mode::Once,
            timeout_seconds: 2,
            interpreter: Some(Interpreter {
                program: std::env::var("ORIEL_TEST_NODE").unwrap_or_else(|_| "node".into()),
                args: vec![],
            }),
        }
        .resolve(None)
        .unwrap()
    }
    fn until(mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        while !check() {
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(15));
        }
    }
    #[test]
    fn results_reject_non_protocol_and_internal_addresses() {
        for line in [
            b"hello".as_slice(),
            br#"{"type":"result","success":true,"url":"http://tauri.localhost"}"#,
            br#"{"type":"other","success":true}"#,
            br#"{"type":"result","success":true,"extra":1}"#,
        ] {
            assert!(parse_result(line).is_err());
        }
        assert!(parse_result(br#"{"type":"result","success":true}"#)
            .unwrap()
            .is_none());
    }
    #[test]
    fn resolves_plugin_paths_and_blocks_escape() {
        let dir = Temp::new();
        let outside = Temp::new();
        fs::write(dir.0.join("prepare.ps1"), "").unwrap();
        fs::write(outside.0.join("outside.ps1"), "").unwrap();
        let mut p: Preparation = serde_json::from_str(r#"{"path":"prepare.ps1"}"#).unwrap();
        assert!(p.resolve(Some(&dir.0)).is_ok());
        assert!(p.validate_local().is_err());
        p.path = outside.0.join("outside.ps1");
        assert!(p.resolve(Some(&dir.0)).is_err());
        std::os::unix::fs::symlink(&outside.0, dir.0.join("link")).unwrap();
        p.path = PathBuf::from("link/outside.ps1");
        assert!(p.resolve(Some(&dir.0)).is_err());
    }
    #[test]
    fn command_defaults_and_invalid_bindings() {
        let p: Preparation = serde_json::from_str(r#"{"path":"C:\\tools\\prepare.ps1"}"#).unwrap();
        assert_eq!(p.mode, Mode::Once);
        assert_eq!(p.timeout_seconds, 120);
        assert!(p.validate_local().is_ok());
        let mut p = p;
        p.path = PathBuf::from("unknown.foo");
        assert!(p.validate().is_err());
        p.runner = Runner::Custom;
        assert!(p.validate().is_err());
        p.interpreter = Some(Interpreter {
            program: "node".into(),
            args: vec![],
        });
        assert!(p.validate_local().is_err());
    }
    #[test]
    fn once_success_and_failure_matrix() {
        let temp = Temp::new();
        for scenario in [
            "once",
            "static",
            "failure",
            "invalid",
            "internal",
            "partial",
            "duplicate",
            "long",
            "utf8",
            "empty",
            "nonzero",
        ] {
            let manager = Preparations::default();
            let p = fixture(scenario, &temp.0.join(scenario));
            let result = manager.prepare("page", "页面", p, false, launch);
            assert_eq!(
                result.is_ok(),
                matches!(scenario, "once" | "static"),
                "{scenario}: {result:?}"
            );
            let state = manager.views().remove(0);
            assert!(state.cleaned);
            assert_ne!(state.state, State::Ready);
        }
    }
    #[test]
    fn arguments_are_passed_literally_to_custom_interpreter() {
        let temp = Temp::new();
        let mut p = fixture("args", &temp.0.join("args"));
        p.args
            .extend(["中文 & % ! ^ spaces".into(), "\"quoted\"".into()]);
        assert!(Preparations::default()
            .prepare("args", "args", p, false, launch)
            .is_ok());
    }
    #[test]
    fn timeout_waits_for_exit_even_after_success_output() {
        let temp = Temp::new();
        for scenario in ["hang", "once-hang"] {
            let manager = Preparations::default();
            let mut p = fixture(scenario, &temp.0.join(scenario));
            p.timeout_seconds = 1;
            assert!(manager
                .prepare("page", "page", p, false, launch)
                .unwrap_err()
                .contains("超时"));
            assert!(manager.views()[0].cleaned);
        }
    }
    #[test]
    fn persistent_reuse_explicit_retry_and_configuration_changes() {
        let temp = Temp::new();
        let manager = Preparations::default();
        let marker = temp.0.join("starts");
        let mut p = fixture("persistent", &marker);
        p.mode = Mode::Persistent;
        manager.prepare("p", "p", p.clone(), false, launch).unwrap();
        manager.prepare("p", "p", p.clone(), false, launch).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
        let mut changed = p.clone();
        changed.timeout_seconds += 1;
        assert!(manager
            .prepare("p", "p", changed, false, launch)
            .unwrap_err()
            .contains("变更"));
        manager.stop("p").unwrap();
        assert_eq!(manager.views()[0].state, State::Stopped);
        assert!(manager.prepare("p", "p", p.clone(), false, launch).is_err());
        manager.prepare("p", "p", p, true, launch).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 2);
        assert!(manager.shutdown().is_empty());
    }
    #[test]
    fn concurrent_requests_share_one_launch_and_pages_are_independent() {
        let temp = Temp::new();
        let manager = Arc::new(Preparations::default());
        let marker = temp.0.join("starts");
        let p = fixture("delayed", &marker);
        let mut workers = vec![];
        for _ in 0..4 {
            let m = manager.clone();
            let p = p.clone();
            workers.push(thread::spawn(move || m.prepare("p", "p", p, false, launch)));
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 1);
        manager.prepare("other", "other", p, false, launch).unwrap();
        assert_eq!(fs::read_to_string(&marker).unwrap().lines().count(), 2);
    }
    #[test]
    fn stop_cancels_preparation_and_retirement_blocks_stale_requests() {
        let temp = Temp::new();
        let manager = Arc::new(Preparations::default());
        let p = fixture("hang", &temp.0.join("hang"));
        let m = manager.clone();
        let p2 = p.clone();
        let task = thread::spawn(move || m.prepare("p", "p", p2, false, launch));
        until(|| !manager.views().is_empty());
        manager.retire("p").unwrap();
        assert!(task.join().unwrap().is_err());
        assert!(manager.prepare("p", "p", p, true, launch).is_err());
        manager.begin_shutdown();
        assert!(manager
            .prepare(
                "new",
                "new",
                fixture("once", &temp.0.join("new")),
                false,
                launch
            )
            .is_err());
    }
    #[test]
    fn persistent_exit_is_a_failure_and_logs_are_bounded() {
        let temp = Temp::new();
        let manager = Preparations::default();
        let mut p = fixture("exit-later", &temp.0.join("exit"));
        p.mode = Mode::Persistent;
        manager.prepare("exit", "exit", p, false, launch).unwrap();
        until(|| manager.views()[0].state == State::Failed);
        assert!(manager.views()[0].cleaned);
        manager
            .prepare(
                "logs",
                "logs",
                fixture("flood", &temp.0.join("logs")),
                false,
                launch,
            )
            .unwrap();
        let views = manager.views();
        let logs = &views.iter().find(|v| v.entry_id == "logs").unwrap().logs;
        assert!(!logs.is_empty());
        assert!(logs.len() <= 256);
        assert!(logs.iter().all(|s| s.len() <= 4096));
    }
    #[test]
    fn rejected_results_are_cleaned_and_old_generations_cannot_cancel_new_runs() {
        let temp = Temp::new();
        let manager = Preparations::default();
        let mut p = fixture("persistent", &temp.0.join("reject"));
        p.mode = Mode::Persistent;
        let first = manager.prepare("p", "p", p.clone(), false, launch).unwrap();
        manager.reject_result("p", first.generation, "网页没有可用地址");
        until(|| manager.views()[0].cleaned);
        assert_eq!(manager.views()[0].state, State::Failed);
        let second = manager.prepare("p", "p", p, true, launch).unwrap();
        assert_ne!(first.generation, second.generation);
        manager.reject_result("p", first.generation, "旧结果");
        assert_eq!(manager.views()[0].state, State::Ready);
        assert!(manager.shutdown().is_empty());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cleanup_reaches_owned_descendants_and_preserves_external_processes() {
        let temp = Temp::new();
        let manager = Preparations::default();
        let node = std::env::var("ORIEL_TEST_NODE").unwrap_or_else(|_| "node".into());
        let mut external = Command::new(node)
            .args(["-e", "setInterval(()=>{},1000)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let marker = temp.0.join("child");
        let mut p = fixture("child", &marker);
        p.mode = Mode::Persistent;
        let result = manager.prepare("p", "p", p, false, launch);
        let child_pid = fs::read_to_string(marker.with_extension("pid")).ok();
        let cleanup = manager.shutdown();
        let external_alive = external.try_wait().unwrap().is_none();
        let _ = external.kill();
        let _ = external.wait();
        result.unwrap();
        assert!(cleanup.is_empty());
        assert!(external_alive);
        let pid = child_pid.unwrap();
        until(|| {
            fs::read_to_string(format!("/proc/{pid}/stat"))
                .map(|s| s.split_once(") ").unwrap().1.starts_with('Z'))
                .unwrap_or(true)
        });
    }
}
