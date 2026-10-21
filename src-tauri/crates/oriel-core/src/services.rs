//! Versioned parent-owned stdio lifecycle; SSH business logic stays in the tool.
use crate::{valid_id, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prerequisite {
    pub plugin_id: String,
    pub rule_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lifecycle {
    pub protocol: String,
    pub entrypoint: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDeclaration {
    pub plugin_id: String,
    pub directory: PathBuf,
    pub lifecycle: Lifecycle,
}
impl ServiceDeclaration {
    pub fn validate(plugin_id: String, directory: &Path, lifecycle: Lifecycle) -> Result<Self> {
        if lifecycle.protocol != "stdio-v1"
            || lifecycle.entrypoint.is_absolute()
            || lifecycle
                .entrypoint
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || lifecycle.entrypoint.extension().and_then(|s| s.to_str()) != Some("ps1")
            || lifecycle
                .args
                .iter()
                .any(|a| a.contains(['\n', '\r', '\0']))
        {
            return Err("生命周期须为 stdio-v1 和插件目录内的 .ps1 入口。".into());
        }
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let entry = directory
            .join(&lifecycle.entrypoint)
            .canonicalize()
            .map_err(|e| format!("托管入口缺失：{e}"))?;
        if !entry.starts_with(&directory) || !entry.is_file() {
            return Err("托管入口必须位于登记目录内。".into());
        }
        let directory = {
            #[cfg(windows)]
            {
                let text = directory.to_string_lossy();
                PathBuf::from(if let Some(t) = text.strip_prefix(r"\\?\UNC\") {
                    format!(r"\\{t}")
                } else {
                    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
                })
            }
            #[cfg(not(windows))]
            {
                directory
            }
        };
        Ok(Self {
            plugin_id,
            directory,
            lifecycle,
        })
    }
}
pub struct Instance {
    child: Child,
    input: Option<ChildStdin>,
    responses: mpsc::Receiver<Value>,
    pub page_url: String,
    pub instance_id: String,
    pub origin: String,
    pub token: String,
}
impl Instance {
    pub fn launch(declaration: &ServiceDeclaration) -> Result<Self> {
        if !cfg!(windows) {
            return Err("本地插件生命周期目前仅支持 Windows。".into());
        }
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(
            declaration
                .directory
                .join(&declaration.lifecycle.entrypoint),
        )
        .args(&declaration.lifecycle.args)
        .current_dir(&declaration.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let child = cmd.spawn().map_err(|e| format!("无法启动插件：{e}"))?;
        Self::from_child(child)
    }
    fn from_child(mut child: Child) -> Result<Self> {
        let diagnostics = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = child.stderr.take() {
            let diagnostics = diagnostics.clone();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut bytes = Vec::new();
                let _ = stderr.by_ref().take(65536).read_to_end(&mut bytes);
                *diagnostics.lock().unwrap() = String::from_utf8_lossy(&bytes).into_owned();
            });
        }
        let input = child.stdin.take();
        let stdout = child.stdout.take().ok_or("插件没有结果通道")?;
        let (tx, rx) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let read =
                    std::io::Read::take(&mut reader, 2 * 1024 * 1024 + 1).read_line(&mut line);
                match read {
                    Ok(0) | Err(_) => break,
                    _ => {
                        if line.len() > 2 * 1024 * 1024 {
                            break;
                        }
                        let value = serde_json::from_str(&line)
                            .unwrap_or(json!({"error":"插件返回非 JSON 数据"}));
                        if tx.send(value).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut instance = Self {
            child,
            input,
            responses: rx,
            page_url: String::new(),
            instance_id: String::new(),
            origin: String::new(),
            token: String::new(),
        };
        let ready = instance
            .responses
            .recv_timeout(Duration::from_secs(20))
            .map_err(|e| format!("插件未报告就绪（{e}）：{}", diagnostics.lock().unwrap()))?;
        if ready["protocol"] != 1 || ready["ready"] != true {
            return Err("插件未报告兼容的真实就绪结果".into());
        }
        instance.instance_id = ready["instance"]
            .as_str()
            .filter(|s| valid_id(s))
            .ok_or("插件实例 ID 无效")?
            .into();
        instance.page_url = ready["pageUrl"].as_str().ok_or("插件未提供页面")?.into();
        let url = url::Url::parse(&instance.page_url).map_err(|_| "插件页面地址无效")?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("托管插件页面须为回环 HTTP 地址".into());
        }
        instance.origin = ready["origin"].as_str().ok_or("插件无控制地址")?.into();
        instance.token = ready["token"].as_str().ok_or("插件无调用身份")?.into();
        if !(32..=128).contains(&instance.token.len())
            || !instance
                .token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("插件调用身份格式无效".into());
        }
        if instance.origin != format!("http://127.0.0.1:{}", url.port().unwrap()) {
            return Err("插件控制与页面来源不一致".into());
        }
        let status = instance.request(json!({"action":"status"}))?;
        if status["instance"] != instance.instance_id {
            return Err("插件实例身份不一致".into());
        }
        Ok(instance)
    }
    pub fn request(&mut self, request: Value) -> Result<Value> {
        if self.child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("插件后台已退出；请显式重试".into());
        }
        let input = self.input.as_mut().ok_or("插件控制通道已关闭")?;
        writeln!(input, "{request}")
            .and_then(|_| input.flush())
            .map_err(|e| e.to_string())?;
        let response = self
            .responses
            .recv_timeout(Duration::from_secs(130))
            .map_err(|_| {
                self.input.take();
                "插件动作超时，控制通道已关闭以取消待执行工作".to_string()
            })?;
        if response["ok"] != true {
            return Err(response["error"].as_str().unwrap_or("插件动作失败").into());
        }
        Ok(response["value"].clone())
    }
    pub fn stop(&mut self) {
        self.input.take();
        for _ in 0..100 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        self.input.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
#[derive(Clone)]
pub struct ServiceHandle {
    control: Arc<Mutex<Instance>>,
    pub page_url: String,
    instance_id: String,
    origin: String,
    token: String,
}
impl ServiceHandle {
    fn request(&self, value: Value) -> Result<Value> {
        self.control
            .lock()
            .map_err(|_| "插件控制锁失败")?
            .request(value)
    }
    fn shutdown(&self) -> Result<()> {
        use std::io::{Read, Write};
        let url = url::Url::parse(&self.origin).map_err(|_| "插件控制地址无效")?;
        let mut stream = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], url.port().ok_or("无控制端口")?)),
            Duration::from_secs(2),
        )
        .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(12)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let body = r#"{"action":"shutdown"}"#;
        write!(stream,"POST /api HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",url.port().unwrap(),self.token,body.len(),body).map_err(|e|e.to_string())?;
        let mut output = String::new();
        stream
            .take(65536)
            .read_to_string(&mut output)
            .map_err(|e| e.to_string())?;
        if !output.contains("200 OK") {
            return Err("插件未确认退出清理".into());
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Services {
    instances: HashMap<String, ServiceHandle>,
    closing: bool,
}
impl Services {
    pub fn start(
        &mut self,
        declarations: &[ServiceDeclaration],
        id: &str,
    ) -> Result<ServiceHandle> {
        if self.closing {
            return Err("Oriel 正在退出".into());
        }
        if !self.instances.contains_key(id) {
            let d = declarations
                .iter()
                .find(|d| d.plugin_id == id)
                .ok_or_else(|| format!("服务提供方 {id} 缺少有效声明或入口"))?;
            let i = Instance::launch(d)?;
            let handle = ServiceHandle {
                page_url: i.page_url.clone(),
                instance_id: i.instance_id.clone(),
                origin: i.origin.clone(),
                token: i.token.clone(),
                control: Arc::new(Mutex::new(i)),
            };
            self.instances.insert(id.into(), handle);
        }
        Ok(self.instances[id].clone())
    }
    pub fn prepare(
        manager: &Arc<Mutex<Self>>,
        declarations: &[ServiceDeclaration],
        entry: &crate::WebEntry,
    ) -> Result<Option<String>> {
        for dependency in &entry.prerequisites {
            let instance = manager
                .lock()
                .map_err(|_| "生命周期锁失败")?
                .start(declarations, &dependency.plugin_id)?;
            let result=instance.request(json!({"action":"ensure","request":{"ruleIds":[dependency.rule_id],"consumer":format!("oriel:{}",entry.id)}}))?;
            if result["instance"] != instance.instance_id || result["ready"] != true {
                return Err(format!(
                    "前置规则 {} 未就绪：{}。可打开提供方页面处理认证后重试。",
                    dependency.rule_id, result
                ));
            }
        }
        entry
            .service
            .as_ref()
            .map(|id| {
                manager
                    .lock()
                    .map_err(|_| "生命周期锁失败")?
                    .start(declarations, id)
                    .map(|i| i.page_url)
            })
            .transpose()
    }
    pub fn shutdown(manager: &Arc<Mutex<Self>>) -> Vec<String> {
        let handles = {
            let mut manager = manager.lock().unwrap();
            manager.closing = true;
            manager
                .instances
                .drain()
                .map(|(_, h)| h)
                .collect::<Vec<_>>()
        };
        let mut errors = vec![];
        for h in handles {
            if let Err(e) = h.shutdown() {
                errors.push(e);
            }
            if let Ok(mut i) = h.control.lock() {
                i.stop();
            }
        }
        errors
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_lifecycle_entry_and_confines_it() {
        let root = std::env::temp_dir().join(crate::unique_id());
        std::fs::create_dir_all(&root).unwrap();
        let make = |p: &str| Lifecycle {
            protocol: "stdio-v1".into(),
            entrypoint: p.into(),
            args: vec![],
        };
        assert!(ServiceDeclaration::validate("test".into(), &root, make("missing.ps1")).is_err());
        assert!(
            ServiceDeclaration::validate("test".into(), &root, make("../outside.ps1")).is_err()
        );
        std::fs::write(root.join("oriel.ps1"), "# fixture").unwrap();
        assert!(ServiceDeclaration::validate("test".into(), &root, make("oriel.ps1")).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
}
