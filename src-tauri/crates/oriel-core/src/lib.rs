//! Platform-independent registration and local configuration for Oriel.
mod plugins;
pub mod services;
pub mod window_state;

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

pub type Result<T> = std::result::Result<T, String>;

pub fn same_web_origin(left: &str, right: &str) -> bool {
    match (Url::parse(left), Url::parse(right)) {
        (Ok(left), Ok(right)) => {
            matches!(left.scheme(), "http" | "https") && left.origin() == right.origin()
        }
        _ => false,
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum ProxyPolicy {
    #[default]
    Inherit,
    System,
    Direct,
    Custom {
        address: String,
    },
}

impl ProxyPolicy {
    /// Explicit endpoint avoids losing HTTP's default port during URL normalization.
    pub fn endpoint(&self) -> Result<Option<String>> {
        self.validate(false)?;
        if let Self::Custom { address } = self {
            let url = Url::parse(address).map_err(|_| "代理地址格式无效。")?;
            Ok(Some(format!(
                "{}://{}:{}",
                url.scheme(),
                url.host_str().unwrap_or(""),
                url.port_or_known_default().ok_or("代理缺少端口。")?
            )))
        } else {
            Ok(None)
        }
    }

    pub fn validate(&self, is_default: bool) -> Result<()> {
        match self {
            Self::Inherit if is_default => Err("默认代理不能继承自身。".into()),
            Self::Custom { address } => {
                let url = Url::parse(address).map_err(|_| "代理地址格式无效。")?;
                if !matches!(url.scheme(), "http" | "socks5")
                    || url.host_str().is_none()
                    || url.port_or_known_default().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.path() != "/" && !url.path().is_empty()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err("代理须为不带账号、路径和参数的 http:// 或 socks5:// 地址；SOCKS5 须指定端口。".into());
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn resolved(&self, default: &Self) -> Self {
        if *self == Self::Inherit {
            default.clone()
        } else {
            self.clone()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    Browser,
    WebPage,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Presentation {
    #[default]
    Browser,
    Application,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebEntry {
    pub id: String,
    pub title: String,
    pub kind: EntryKind,
    #[serde(default)]
    pub presentation: Presentation,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub proxy: ProxyPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prerequisites: Vec<services::Prerequisite>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub entries: Vec<WebEntry>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub lifecycle: Option<services::Lifecycle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub version: u32,
    #[serde(default)]
    pub plugin_directories: Vec<PathBuf>,
    #[serde(default)]
    pub plugin_roots: Vec<PathBuf>,
    #[serde(default)]
    pub web_entries: Vec<WebEntry>,
    #[serde(default = "system_proxy")]
    pub default_proxy: ProxyPolicy,
    #[serde(default)]
    pub browser_sidebar: BrowserSidebar,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserSidebar {
    pub collapsed: bool,
    pub width: u16,
}

impl Default for BrowserSidebar {
    fn default() -> Self {
        Self {
            collapsed: false,
            width: 240,
        }
    }
}

fn system_proxy() -> ProxyPolicy {
    ProxyPolicy::System
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 2,
            plugin_directories: vec![],
            plugin_roots: vec![],
            web_entries: vec![],
            default_proxy: system_proxy(),
            browser_sidebar: BrowserSidebar::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredEntry {
    #[serde(flatten)]
    pub entry: WebEntry,
    pub editable: bool,
    pub plugin_name: String,
    pub documentation: Option<String>,
    #[serde(skip)]
    pub directory: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub entries: Vec<RegisteredEntry>,
    pub default_proxy: ProxyPolicy,
    pub plugin_directories: Vec<PathBuf>,
    pub plugin_roots: Vec<PathBuf>,
    pub default_plugin_root: PathBuf,
    pub diagnostics: Vec<String>,
    pub writable: bool,
    pub browser_sidebar: BrowserSidebar,
    pub services: Vec<services::ServiceDeclaration>,
}

pub fn valid_id(id: &str) -> bool {
    let stem = id.split('.').next().unwrap_or("");
    let reserved = matches!(stem, "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    !id.is_empty()
        && id.len() <= 128
        && !reserved
        && !id.ends_with('.')
        && (id.as_bytes()[0].is_ascii_lowercase() || id.as_bytes()[0].is_ascii_digit())
        && id.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        })
}

/// Only actual web URLs are navigable; application resources never enter a remote view.
pub fn normalize_url(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() || input.len() > 8192 || input.chars().any(char::is_control) {
        return Err("请输入有效的网页地址。".into());
    }
    let unqualified = !input.contains("://");
    let candidate = if !unqualified {
        input.to_owned()
    } else {
        // Accept host:port, but never turn a script/file scheme into a web URL.
        let authority = input.split(['/', '?', '#']).next().unwrap_or("");
        if let Some((_, port)) = authority.split_once(':') {
            if !authority.starts_with('[')
                && (port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()))
            {
                return Err("网页地址无效，仅支持 HTTP 和 HTTPS。".into());
            }
        }
        format!("https://{input}")
    };
    let mut url = Url::parse(&candidate).map_err(|_| "网页地址格式无效。")?;
    if unqualified
        && (url.host_str() == Some("localhost")
            || matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
            || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback()))
    {
        // Reparse so an explicit :443 is not lost as an HTTPS default.
        url = Url::parse(&format!("http://{input}")).map_err(|_| "网页地址格式无效。")?;
    }
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("仅支持不含账号密码的 HTTP 和 HTTPS 网页地址。".into());
    }
    let host = url.host_str().unwrap_or("");
    if host == "tauri.localhost"
        || host.ends_with(".tauri.localhost")
        || host == "asset.localhost"
        || host.ends_with(".asset.localhost")
        || host == "ipc.localhost"
        || host.ends_with(".ipc.localhost")
        || matches!(host, "localhost" | "127.0.0.1" | "[::1]") && url.port() == Some(1420)
    {
        return Err("不能在网页容器中打开宿主内部地址。".into());
    }
    Ok(url.to_string())
}

fn validate_entry(entry: &mut WebEntry) -> Result<()> {
    if entry.kind == EntryKind::Browser && entry.presentation != Presentation::Browser {
        return Err("浏览器工作区不能使用工具应用布局。".into());
    }
    if !valid_id(&entry.id) {
        return Err("入口 ID 无效。".into());
    }
    entry.title = entry.title.trim().to_owned();
    if entry.title.is_empty() || entry.title.chars().count() > 80 {
        return Err("网页名称须为 1–80 个字符。".into());
    }
    entry.proxy.validate(false)?;
    for dependency in &entry.prerequisites {
        if !valid_id(&dependency.plugin_id) || !valid_id(&dependency.rule_id) {
            return Err("前置资源须引用有效的插件及稳定规则 ID。".into());
        }
    }
    if entry.service.is_some() && entry.url.is_some() {
        return Err("服务页面地址应由后台返回，不能同时声明固定 URL。".into());
    }
    if entry.service.as_deref().is_some_and(|id| !valid_id(id)) {
        return Err("页面服务提供方 ID 无效。".into());
    }
    if entry.kind == EntryKind::WebPage && entry.service.is_none() {
        entry.url = Some(normalize_url(entry.url.as_deref().unwrap_or(""))?);
    } else if entry.kind == EntryKind::Browser
        && (entry.url.is_some() || entry.service.is_some() || !entry.prerequisites.is_empty())
    {
        return Err("浏览器插件入口不能声明固定网址。".into());
    }
    Ok(())
}

fn validate_workspace(workspace: &mut Workspace) -> Result<()> {
    if workspace.version != 2 {
        return Err("不支持此配置版本，原文件已保留。".into());
    }
    if !(200..=360).contains(&workspace.browser_sidebar.width) {
        return Err("浏览器侧栏宽度须为 200–360。".into());
    }
    workspace.default_proxy.validate(true)?;
    let mut ids = HashSet::new();
    for entry in &mut workspace.web_entries {
        validate_entry(entry)?;
        if entry.kind != EntryKind::WebPage
            || entry.presentation != Presentation::Browser
            || !entry.id.starts_with("user.")
            || !ids.insert(entry.id.clone())
        {
            return Err("自定义网页的类型或唯一 ID 无效。".into());
        }
    }
    if workspace
        .plugin_directories
        .iter()
        .chain(&workspace.plugin_roots)
        .any(|p| !p.is_absolute())
    {
        return Err("插件目录和读取目录须使用当前设备的绝对路径。".into());
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path).map_err(|e| format!("读取配置失败：{e}"))?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("配置文件超过 2 MiB，未加载。".into());
    }
    fs::read_to_string(path).map_err(|e| format!("读取配置失败：{e}"))
}

pub fn unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

pub struct ConfigStore {
    path: PathBuf,
    pub workspace: Workspace,
    load_error: Option<String>,
}

impl ConfigStore {
    pub fn open(path: PathBuf) -> Self {
        let result = if path.exists() {
            read_json(&path)
                .and_then(|text| {
                    serde_json::from_str::<Workspace>(&text)
                        .map_err(|_| "本地配置格式无效，原文件已保留。".into())
                })
                .and_then(|mut value| {
                    // Upgrade in memory only. A failed read or write must preserve the old file.
                    if value.version == 1 {
                        value.version = 2;
                    }
                    validate_workspace(&mut value)?;
                    Ok(value)
                })
        } else {
            Ok(Workspace::default())
        };
        match result {
            Ok(workspace) => Self {
                path,
                workspace,
                load_error: None,
            },
            Err(error) => Self {
                path,
                workspace: Workspace::default(),
                load_error: Some(error),
            },
        }
    }

    pub fn commit(&mut self, mut next: Workspace) -> Result<()> {
        if self.load_error.is_some() {
            return Err("本地配置无法读取；请修复或移走原文件后重新加载，避免覆盖。".into());
        }
        validate_workspace(&mut next)?;
        let parent = self.path.parent().ok_or("配置目录无效。")?;
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
        let pending = parent.join(format!("workspace-{}.tmp", unique_id()));
        let write = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pending)
                .map_err(|e| e.to_string())?;
            let data = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
            if data.len() > 2 * 1024 * 1024 {
                return Err("配置超过 2 MiB，未保存。".into());
            }
            file.write_all(&data)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            fs::rename(&pending, &self.path).map_err(|e| e.to_string())
        })();
        if let Err(error) = write {
            let _ = fs::remove_file(pending);
            return Err(format!("保存配置失败：{error}"));
        }
        self.workspace = next;
        Ok(())
    }

    pub fn snapshot(&self, builtin: &str) -> Snapshot {
        let default_plugin_root = self.path.parent().unwrap_or(Path::new(".")).join("plugins");
        let mut roots = vec![default_plugin_root.clone()];
        roots.extend(self.workspace.plugin_roots.clone());
        let (directories, discovery_errors) =
            plugins::discover(&self.workspace.plugin_directories, &roots);
        let mut snapshot = Snapshot {
            entries: vec![],
            default_proxy: self.workspace.default_proxy.clone(),
            plugin_directories: self.workspace.plugin_directories.clone(),
            plugin_roots: self.workspace.plugin_roots.clone(),
            default_plugin_root,
            diagnostics: self.load_error.iter().cloned().collect(),
            writable: self.load_error.is_none(),
            browser_sidebar: self.workspace.browser_sidebar.clone(),
            services: vec![],
        };
        snapshot.diagnostics.extend(discovery_errors);
        let mut plugin_ids = HashSet::new();
        let mut entry_ids = HashSet::new();
        let mut register = |text: &str, builtin: bool, directory: Option<&Path>| -> Result<()> {
            let mut plugin: PluginManifest =
                serde_json::from_str(text).map_err(|_| "插件声明格式无效。")?;
            if ![1, 2].contains(&plugin.schema_version)
                || !valid_id(&plugin.id)
                || plugin.id == "user"
                || plugin.id.starts_with("user.")
                || plugin.name.trim().is_empty()
                || plugin.version.trim().is_empty()
                || if plugin.lifecycle.is_some() {
                    plugin.schema_version != 2
                        || plugin.capabilities != ["web-page", "local-service-v1"]
                } else {
                    plugin.capabilities != ["web-page"]
                }
                || plugin.schema_version == 1
                    && (plugin.lifecycle.is_some()
                        || plugin.documentation.is_some()
                        || plugin.entries.iter().any(|e| {
                            e.service.is_some()
                                || !e.prerequisites.is_empty()
                                || e.presentation != Presentation::Browser
                        }))
                || plugin_ids.contains(&plugin.id)
            {
                return Err("插件身份、声明版本或能力无效／重复。".into());
            }
            if let Some(documentation) = &plugin.documentation {
                plugins::read_documentation(
                    directory.ok_or("内置声明不读取外部文档。")?,
                    documentation,
                )?;
            }
            let service = if let Some(lifecycle) = &plugin.lifecycle {
                let directory = directory.ok_or("内置声明不启动外部脚本。")?;
                Some(services::ServiceDeclaration::validate(
                    plugin.id.clone(),
                    directory,
                    lifecycle.clone(),
                )?)
            } else {
                None
            };
            let mut pending_ids = HashSet::new();
            for entry in &mut plugin.entries {
                validate_entry(entry)?;
                if !builtin && entry.kind != EntryKind::WebPage {
                    return Err("外部声明目前仅支持 web-page 入口。".into());
                }
                entry.id = format!("{}.{}", plugin.id, entry.id);
                if !valid_id(&entry.id)
                    || entry_ids.contains(&entry.id)
                    || !pending_ids.insert(entry.id.clone())
                {
                    return Err("插件入口 ID 重复或过长。".into());
                }
            }
            if let Some(service) = service {
                snapshot.services.push(service);
            }
            plugin_ids.insert(plugin.id);
            entry_ids.extend(pending_ids);
            snapshot
                .entries
                .extend(plugin.entries.into_iter().map(|entry| RegisteredEntry {
                    entry,
                    editable: false,
                    plugin_name: plugin.name.clone(),
                    documentation: plugin.documentation.clone(),
                    directory: directory.map(Path::to_path_buf),
                }));
            Ok(())
        };
        if let Err(error) = register(builtin, true, None) {
            snapshot.diagnostics.push(format!("内置浏览器：{error}"));
        }
        for directory in &directories {
            match read_json(&directory.join("plugin.json"))
                .and_then(|text| register(&text, false, Some(directory)))
            {
                Ok(()) => (),
                Err(error) => snapshot
                    .diagnostics
                    .push(format!("插件 {}：{error}", directory.display())),
            }
        }
        snapshot
            .entries
            .extend(
                self.workspace
                    .web_entries
                    .iter()
                    .cloned()
                    .map(|entry| RegisteredEntry {
                        entry,
                        editable: true,
                        plugin_name: "浏览器".into(),
                        documentation: None,
                        directory: None,
                    }),
            );
        snapshot
    }

    pub fn plugin_documentation(&self, builtin: &str, entry_id: &str) -> Result<String> {
        let entry = self
            .snapshot(builtin)
            .entries
            .into_iter()
            .find(|entry| entry.entry.id == entry_id)
            .ok_or("插件入口不存在，请重新加载接入。")?;
        plugins::read_documentation(
            entry.directory.as_deref().ok_or("此入口未提供插件说明。")?,
            entry
                .documentation
                .as_deref()
                .ok_or("此插件未提供说明文档。")?,
        )
    }

    pub fn save_entry(
        &mut self,
        id: Option<String>,
        title: String,
        url: String,
        proxy: ProxyPolicy,
    ) -> Result<String> {
        let mut next = self.workspace.clone();
        let id = id.unwrap_or_else(|| format!("user.{}", unique_id()));
        let mut entry = WebEntry {
            id: id.clone(),
            title,
            kind: EntryKind::WebPage,
            presentation: Presentation::Browser,
            url: Some(url),
            proxy,
            service: None,
            prerequisites: vec![],
        };
        if let Some(old) = next.web_entries.iter_mut().find(|e| e.id == id) {
            entry.service = old.service.clone();
            entry.prerequisites = old.prerequisites.clone();
            *old = entry;
        } else {
            if !id.starts_with("user.") {
                return Err("只能编辑自定义网页。".into());
            }
            next.web_entries.push(entry);
        }
        self.commit(next)?;
        Ok(id)
    }

    pub fn delete_entry(&mut self, id: &str) -> Result<()> {
        let mut next = self.workspace.clone();
        let before = next.web_entries.len();
        next.web_entries.retain(|e| e.id != id);
        if next.web_entries.len() == before {
            return Err("自定义网页不存在。".into());
        }
        self.commit(next)
    }

    pub fn reorder_entries(&mut self, ids: Vec<String>) -> Result<()> {
        let requested: HashSet<_> = ids.iter().collect();
        let existing: HashSet<_> = self
            .workspace
            .web_entries
            .iter()
            .map(|entry| &entry.id)
            .collect();
        if ids.len() != existing.len() || requested != existing {
            return Err("网页列表已变化，请重新加载后再排序。".into());
        }
        let mut next = self.workspace.clone();
        let mut entries: HashMap<_, _> = next
            .web_entries
            .drain(..)
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        next.web_entries = ids.iter().map(|id| entries.remove(id).unwrap()).collect();
        self.commit(next)
    }

    pub fn save_browser_sidebar(&mut self, sidebar: BrowserSidebar) -> Result<()> {
        let mut next = self.workspace.clone();
        next.browser_sidebar = sidebar;
        self.commit(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const BUILTIN: &str = include_str!("../../../../plugins/builtin/browser/plugin.json");

    fn write_plugin(directory: &Path, id: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(directory.join("plugin.json"), serde_json::json!({
            "schemaVersion": 2, "id": id, "name": id, "version": "1",
            "capabilities": ["web-page"], "documentation": "guide.md",
            "entries": [{"id":"main","title":"Tool","kind":"web-page","presentation":"application","url":"http://127.0.0.1:8080"}]
        }).to_string()).unwrap();
        fs::write(directory.join("guide.md"), "# 使用说明\n\n配置和运行分开。").unwrap();
    }

    #[test]
    fn discovers_applications_without_registration_or_script_execution() {
        let dir = TestDir::new();
        let plugin = dir.0.join("plugins/tool");
        write_plugin(&plugin, "example.tool");
        fs::write(
            plugin.join("oriel.ps1"),
            "throw 'must not execute during discovery'",
        )
        .unwrap();
        // Files in grandchildren are not independently discovered.
        write_plugin(&plugin.join("nested"), "example.nested");
        let store = ConfigStore::open(dir.config());
        let snapshot = store.snapshot(BUILTIN);
        assert!(
            snapshot.diagnostics.is_empty(),
            "{:?}",
            snapshot.diagnostics
        );
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(
            snapshot.entries[1].entry.presentation,
            Presentation::Application
        );
        assert_eq!(snapshot.default_plugin_root, dir.0.join("plugins"));
        assert!(store
            .plugin_documentation(BUILTIN, "example.tool.main")
            .unwrap()
            .contains("配置和运行分开"));
        fs::remove_file(plugin.join("plugin.json")).unwrap();
        let refreshed = store.snapshot(BUILTIN);
        assert_eq!(refreshed.entries.len(), 1);
        assert_eq!(refreshed.diagnostics.len(), 1);
    }

    #[test]
    fn discovery_deduplicates_paths_and_reports_duplicate_ids_and_bad_manifests() {
        let dir = TestDir::new();
        let first = dir.0.join("plugins/first");
        write_plugin(&first, "example.tool");
        write_plugin(&dir.0.join("plugins/second"), "example.tool");
        fs::create_dir_all(dir.0.join("plugins/broken")).unwrap();
        let extra = dir.0.join("additional");
        write_plugin(&extra.join("extra"), "example.extra");
        let mut store = ConfigStore::open(dir.config());
        let mut next = store.workspace.clone();
        next.plugin_directories.push(first.clone());
        next.plugin_roots = vec![extra.clone(), extra];
        store.commit(next).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 3);
        assert_eq!(snapshot.entries[1].directory.as_ref(), Some(&first));
        assert_eq!(snapshot.diagnostics.len(), 2, "{:?}", snapshot.diagnostics);
        assert_eq!(
            ConfigStore::open(dir.config()).workspace.plugin_roots.len(),
            2
        );
        let mut next = store.workspace.clone();
        next.plugin_roots.push(dir.0.join("missing-root"));
        store.commit(next).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 3);
        assert!(snapshot
            .diagnostics
            .iter()
            .any(|message| message.contains("missing-root")));
    }

    #[test]
    fn documents_are_bounded_utf8_files_confined_to_the_plugin_directory() {
        let dir = TestDir::new();
        let plugin = dir.0.join("tool");
        write_plugin(&plugin, "example.tool");
        fs::write(dir.0.join("outside.md"), "outside").unwrap();
        for relative in [
            "../outside.md",
            "/outside.md",
            "C:/outside.md",
            "..\\outside.md",
            "guide.html",
        ] {
            assert!(
                plugins::read_documentation(&plugin, relative).is_err(),
                "{relative}"
            );
        }
        fs::write(plugin.join("large.md"), vec![b'a'; 256 * 1024 + 1]).unwrap();
        fs::write(plugin.join("invalid.md"), [0xff, 0xfe]).unwrap();
        assert!(plugins::read_documentation(&plugin, "large.md").is_err());
        assert!(plugins::read_documentation(&plugin, "invalid.md").is_err());
        assert!(plugins::read_documentation(&plugin, "missing.md").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.0.join("outside.md"), plugin.join("link.md")).unwrap();
            assert!(plugins::read_documentation(&plugin, "link.md").is_err());
            std::os::unix::fs::symlink(&plugin, dir.0.join("linked-tool")).unwrap();
            let (discovered, _) = plugins::discover(&[], std::slice::from_ref(&dir.0));
            assert!(!discovered.contains(&dir.0.join("linked-tool")));
        }
    }

    #[test]
    fn rejects_unknown_presentations_and_keeps_legacy_browser_defaults() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        assert_eq!(
            store.snapshot(BUILTIN).entries[0].entry.presentation,
            Presentation::Browser
        );
        let mut next = store.workspace.clone();
        next.plugin_roots.push(PathBuf::from("relative"));
        assert!(store.commit(next).is_err());
        let plugin = dir.0.join("plugins/tool");
        write_plugin(&plugin, "example.tool");
        let path = plugin.join("plugin.json");
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace("application", "unknown")).unwrap();
        assert_eq!(store.snapshot(BUILTIN).entries.len(), 1);
        fs::write(
            &path,
            text.replace("\"schemaVersion\":2", "\"schemaVersion\":1"),
        )
        .unwrap();
        assert_eq!(store.snapshot(BUILTIN).entries.len(), 1);
        fs::write(&path, text).unwrap();
        assert_eq!(store.snapshot(BUILTIN).entries.len(), 2);
    }

    struct TestDir(PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("oriel-core-{}", unique_id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> PathBuf {
            self.0.join("workspace.json")
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn accepts_public_and_local_websites() {
        assert_eq!(
            normalize_url("example.com").unwrap(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url("localhost:3000/login").unwrap(),
            "http://localhost:3000/login"
        );
        assert_eq!(
            normalize_url("http://[::1]:8080").unwrap(),
            "http://[::1]:8080/"
        );
        assert_eq!(
            normalize_url("localhost:3000?next=/a:b").unwrap(),
            "http://localhost:3000/?next=/a:b"
        );
        assert_eq!(
            normalize_url("localhost.example.com/path:a").unwrap(),
            "https://localhost.example.com/path:a"
        );
        assert_eq!(
            normalize_url("localhost:443").unwrap(),
            "http://localhost:443/"
        );
    }
    #[test]
    fn identifiers_cannot_alias_windows_profile_directories() {
        for id in [
            "Example.page",
            "example.page.",
            "con.page",
            "lpt1.page",
            "../page",
            "",
        ] {
            assert!(!valid_id(id), "{id}");
        }
        assert!(valid_id("example.my-page_1"));
    }
    #[test]
    fn rejects_internal_schemes_and_credentials() {
        for url in [
            "javascript:alert(1)",
            "data:text/html,test",
            "file:///etc/passwd",
            "https://user:pass@example.com",
            "http://tauri.localhost",
            "http://asset.localhost",
            "http://127.0.0.1:1420",
            "",
        ] {
            assert!(normalize_url(url).is_err(), "{url}");
        }
    }
    #[test]
    fn proxy_validation_does_not_silently_drop_authentication() {
        for address in [
            "http://user:secret@localhost:8080",
            "socks5://localhost",
            "https://localhost:8080",
            "http://localhost:8080/path",
            "http://localhost:8080?x=1",
        ] {
            assert!(ProxyPolicy::Custom {
                address: address.into()
            }
            .validate(false)
            .is_err());
        }
        assert!(ProxyPolicy::Custom {
            address: "socks5://127.0.0.1:1080".into()
        }
        .validate(false)
        .is_ok());
    }
    #[test]
    fn proxy_inheritance_preserves_explicit_direct_mode() {
        let default = ProxyPolicy::Custom {
            address: "http://127.0.0.1:7890".into(),
        };
        assert_eq!(ProxyPolicy::Inherit.resolved(&default), default);
        assert_eq!(ProxyPolicy::Direct.resolved(&default), ProxyPolicy::Direct);
        assert!(ProxyPolicy::Inherit.validate(true).is_err());
    }
    #[test]
    fn proxy_endpoints_keep_default_ports_and_ipv6_brackets() {
        for (input, expected) in [
            ("http://localhost", "http://localhost:80"),
            ("http://localhost:80", "http://localhost:80"),
            ("socks5://[::1]:1080", "socks5://[::1]:1080"),
        ] {
            assert_eq!(
                ProxyPolicy::Custom {
                    address: input.into()
                }
                .endpoint()
                .unwrap()
                .as_deref(),
                Some(expected)
            );
        }
    }
    #[test]
    fn registration_and_restart_preserve_identity_and_settings() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        let id = store
            .save_entry(
                None,
                "工具".into(),
                "localhost:3000".into(),
                ProxyPolicy::Direct,
            )
            .unwrap();
        store
            .save_entry(
                Some(id.clone()),
                "新名称".into(),
                "example.com".into(),
                ProxyPolicy::Inherit,
            )
            .unwrap();
        let reopened = ConfigStore::open(dir.config());
        let snapshot = reopened.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.entries[1].entry.id, id);
        assert_eq!(
            snapshot.entries[1].entry.url.as_deref(),
            Some("https://example.com/")
        );
        store.delete_entry(&id).unwrap();
        assert_eq!(
            ConfigStore::open(dir.config())
                .snapshot(BUILTIN)
                .entries
                .len(),
            1
        );
    }
    #[test]
    fn corrupt_and_future_configs_remain_intact() {
        for contents in ["broken", "{\"version\":99}"] {
            let dir = TestDir::new();
            fs::write(dir.config(), contents).unwrap();
            let mut store = ConfigStore::open(dir.config());
            assert!(!store.snapshot(BUILTIN).writable);
            assert!(store.commit(Workspace::default()).is_err());
            assert_eq!(fs::read_to_string(dir.config()).unwrap(), contents);
        }
    }
    #[test]
    fn invalid_external_plugin_does_not_remove_working_entries() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        let mut next = store.workspace.clone();
        next.plugin_directories.push(dir.0.clone());
        store.commit(next).unwrap();
        fs::write(dir.0.join("plugin.json"), r#"{"schemaVersion":1,"id":"oriel.browser","name":"fake","version":"1","capabilities":["web-page"],"entries":[]}"#).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.diagnostics.len(), 1);
    }
    #[test]
    fn external_webpage_manifest_is_loaded_without_compilation() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        let mut next = store.workspace.clone();
        next.plugin_directories.push(dir.0.clone());
        store.commit(next).unwrap();
        fs::write(dir.0.join("plugin.json"), r#"{"schemaVersion":1,"id":"example.tools","name":"示例","version":"1","capabilities":["web-page"],"entries":[{"id":"site","title":"工具","kind":"web-page","url":"http://localhost:3000"}]}"#).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries[1].entry.id, "example.tools.site");
        assert!(!snapshot.entries[1].editable);
        assert!(snapshot.diagnostics.is_empty());
    }
    #[test]
    fn duplicate_ids_reject_entire_manifest() {
        let dir = TestDir::new();
        let store = ConfigStore::open(dir.config());
        let invalid = BUILTIN.replace(
            "\"entries\": [",
            "\"entries\": [{\"id\":\"browser\",\"title\":\"重复\",\"kind\":\"browser\"},",
        );
        let snapshot = store.snapshot(&invalid);
        assert!(snapshot.entries.is_empty());
        assert_eq!(snapshot.diagnostics.len(), 1);
    }
    #[test]
    fn legacy_workspace_migrates_without_changing_webpage_identity() {
        let dir = TestDir::new();
        let legacy = r#"{"version":1,"webEntries":[{"id":"user.existing","title":"工作邮箱","kind":"web-page","url":"https://example.com/","proxy":{"mode":"direct"}}],"defaultProxy":{"mode":"system"}}"#;
        fs::write(dir.config(), legacy).unwrap();
        let mut store = ConfigStore::open(dir.config());
        assert!(store.snapshot(BUILTIN).writable);
        assert_eq!(store.workspace.version, 2);
        assert_eq!(store.workspace.browser_sidebar, BrowserSidebar::default());
        assert_eq!(fs::read_to_string(dir.config()).unwrap(), legacy);
        store
            .save_browser_sidebar(BrowserSidebar {
                collapsed: true,
                width: 280,
            })
            .unwrap();
        let restored = ConfigStore::open(dir.config());
        assert_eq!(restored.workspace.web_entries[0].id, "user.existing");
        assert_eq!(restored.workspace.web_entries[0].proxy, ProxyPolicy::Direct);
        assert!(restored.workspace.browser_sidebar.collapsed);
        assert_eq!(restored.workspace.browser_sidebar.width, 280);
        assert_eq!(restored.workspace.version, 2);
    }
    #[test]
    fn reorder_preserves_entries_and_rejects_stale_or_duplicate_ids() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        let first = store
            .save_entry(None, "一".into(), "example.com".into(), ProxyPolicy::Direct)
            .unwrap();
        let second = store
            .save_entry(
                None,
                "二".into(),
                "example.org".into(),
                ProxyPolicy::Inherit,
            )
            .unwrap();
        store
            .reorder_entries(vec![second.clone(), first.clone()])
            .unwrap();
        let restored = ConfigStore::open(dir.config());
        assert_eq!(restored.workspace.web_entries[0].id, second);
        assert_eq!(restored.workspace.web_entries[1].id, first);
        assert_eq!(restored.workspace.web_entries[1].proxy, ProxyPolicy::Direct);
        let before = fs::read(dir.config()).unwrap();
        for ids in [
            vec![first.clone()],
            vec![first.clone(), first.clone()],
            vec![first, "user.unknown".into()],
        ] {
            assert!(store.reorder_entries(ids).is_err());
            assert_eq!(fs::read(dir.config()).unwrap(), before);
        }
    }
    #[test]
    fn invalid_browser_sidebar_cannot_overwrite_config() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        store.commit(Workspace::default()).unwrap();
        let before = fs::read(dir.config()).unwrap();
        for width in [0, 199, 361, u16::MAX] {
            assert!(store
                .save_browser_sidebar(BrowserSidebar {
                    collapsed: false,
                    width
                })
                .is_err());
            assert_eq!(fs::read(dir.config()).unwrap(), before);
        }
        let invalid = String::from_utf8(before).unwrap().replace("240", "999");
        fs::write(dir.config(), &invalid).unwrap();
        let mut invalid_store = ConfigStore::open(dir.config());
        assert!(!invalid_store.snapshot(BUILTIN).writable);
        assert!(invalid_store
            .save_browser_sidebar(BrowserSidebar::default())
            .is_err());
        assert_eq!(fs::read_to_string(dir.config()).unwrap(), invalid);
    }
    #[test]
    fn invalid_edits_do_not_modify_persisted_config() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        store
            .save_entry(
                None,
                "有效".into(),
                "example.com".into(),
                ProxyPolicy::Inherit,
            )
            .unwrap();
        let before = fs::read(dir.config()).unwrap();
        assert!(store
            .save_entry(
                None,
                "无效".into(),
                "javascript:alert(1)".into(),
                ProxyPolicy::Inherit
            )
            .is_err());
        assert_eq!(fs::read(dir.config()).unwrap(), before);
        assert_eq!(store.workspace.web_entries.len(), 1);
    }
    #[test]
    fn service_execution_requires_explicit_capability_and_valid_entrypoint() {
        let dir = TestDir::new();
        fs::write(dir.0.join("oriel.ps1"), "# isolated fixture").unwrap();
        let mut store = ConfigStore::open(dir.config());
        let mut next = store.workspace.clone();
        next.plugin_directories = vec![dir.0.clone()];
        store.commit(next).unwrap();
        let mut manifest = serde_json::json!({"schemaVersion":2,"id":"example.service","name":"service","version":"1","capabilities":["web-page"],"lifecycle":{"protocol":"stdio-v1","entrypoint":"oriel.ps1"},"entries":[{"id":"page","title":"工具","kind":"web-page","service":"example.service"}]});
        fs::write(dir.0.join("plugin.json"), manifest.to_string()).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.diagnostics.is_empty());
        assert!(snapshot.services.is_empty());
        manifest["capabilities"] = serde_json::json!(["web-page", "local-service-v1"]);
        fs::write(dir.0.join("plugin.json"), manifest.to_string()).unwrap();
        let snapshot = store.snapshot(BUILTIN);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.services.len(), 1);
        assert!(snapshot.diagnostics.is_empty());
        manifest["lifecycle"]["entrypoint"] = serde_json::json!("missing.ps1");
        fs::write(dir.0.join("plugin.json"), manifest.to_string()).unwrap();
        assert_eq!(store.snapshot(BUILTIN).entries.len(), 1);
    }
    #[test]
    fn webpage_edits_preserve_stable_resource_references() {
        let dir = TestDir::new();
        let mut store = ConfigStore::open(dir.config());
        let id = store
            .save_entry(
                None,
                "页面".into(),
                "http://localhost:18080".into(),
                ProxyPolicy::Direct,
            )
            .unwrap();
        let mut next = store.workspace.clone();
        next.web_entries[0].prerequisites = vec![services::Prerequisite {
            plugin_id: "tether".into(),
            rule_id: "ops-web".into(),
        }];
        store.commit(next).unwrap();
        store
            .save_entry(
                Some(id),
                "改名".into(),
                "http://localhost:18080/new".into(),
                ProxyPolicy::Direct,
            )
            .unwrap();
        assert_eq!(
            store.workspace.web_entries[0].prerequisites[0].rule_id,
            "ops-web"
        );
    }
}
