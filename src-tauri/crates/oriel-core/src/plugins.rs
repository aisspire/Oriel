//! Declarative discovery and bounded, plugin-local documentation. No code is executed here.
use crate::Result;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub fn discover(explicit: &[PathBuf], roots: &[PathBuf]) -> (Vec<PathBuf>, Vec<String>) {
    let mut candidates = explicit.to_vec();
    let mut diagnostics = vec![];
    for (index, root) in roots.iter().enumerate() {
        let children = match fs::read_dir(root) {
            Ok(children) => children,
            Err(error) if index == 0 && error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                diagnostics.push(format!("插件读取目录 {}：{error}", root.display()));
                continue;
            }
        };
        let mut directories = vec![];
        for child in children {
            match child.and_then(|child| child.file_type().map(|kind| (child.path(), kind))) {
                // Do not traverse directory links or recurse into source/build trees.
                Ok((path, kind)) if kind.is_dir() => directories.push(path),
                Ok(_) => (),
                Err(error) => diagnostics.push(format!("插件读取目录 {}：{error}", root.display())),
            }
        }
        directories.sort();
        candidates.extend(directories);
    }
    let mut seen = HashSet::new();
    candidates.retain(|path| seen.insert(fs::canonicalize(path).unwrap_or_else(|_| path.clone())));
    (candidates, diagnostics)
}

pub fn read_documentation(directory: &Path, relative: &str) -> Result<String> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        || !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("md" | "txt")
        )
    {
        return Err("说明文档须为插件目录内的相对 .md 或 .txt 路径。".into());
    }
    let root = fs::canonicalize(directory).map_err(|e| format!("插件目录不可读：{e}"))?;
    let file = fs::canonicalize(root.join(path)).map_err(|e| format!("说明文档不可读：{e}"))?;
    if !file.starts_with(&root) {
        return Err("说明文档不能指向插件目录之外。".into());
    }
    const LIMIT: u64 = 256 * 1024;
    let file = fs::File::open(file).map_err(|e| format!("说明文档不可读：{e}"))?;
    if !file
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= LIMIT)
    {
        return Err("说明文档须为不超过 256 KiB 的 UTF-8 文件。".into());
    }
    let mut contents = String::new();
    file.take(LIMIT + 1)
        .read_to_string(&mut contents)
        .map_err(|e| format!("说明文档读取失败：{e}"))?;
    if contents.len() as u64 > LIMIT {
        return Err("说明文档超过 256 KiB。".into());
    }
    Ok(contents)
}
