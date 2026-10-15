//! 账号迁移：导出/导入 tar 包（快照制专属，区别于参考项目的 JSON 凭据导出）。
//!
//! 包格式（无压缩 tar）：
//! ```text
//! export.json                 清单（format/version/exportedAt/accounts 元数据）
//! accounts/<id>/...          账号快照目录（登录态文件本体，凭据就在这里）
//! ```
//!
//! 合规：本模块只搬运整文件，全程不读取/打印文件内容；导出是用户显式动作，
//! 前端必须明确提示「该文件包含登录凭据，请妥善保管」。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::modules::account::{account_meta, load_accounts, save_accounts};
use crate::modules::config::{now_ms, snapshots_dir};

const BUNDLE_FORMAT: &str = "tt-switch-account-bundle";
const BUNDLE_VERSION: u32 = 1;
const MANIFEST_ENTRY: &str = "export.json";
const ACCOUNTS_PREFIX: &str = "accounts/";

/// 导出账号到 tar 包。`ids` 为空表示导出全部。
pub fn export_accounts(ids: &[String], dest: &str) -> Result<Value, String> {
    let all = load_accounts();
    let selected: Vec<Value> = if ids.is_empty() {
        all.clone()
    } else {
        ids.iter()
            .map(|id| {
                all.iter()
                    .find(|a| a.get("id").and_then(Value::as_str) == Some(id.as_str()))
                    .cloned()
                    .ok_or_else(|| format!("账号不存在：{id}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if selected.is_empty() {
        return Err("没有可导出的账号".to_string());
    }

    let dest = dest.trim();
    let dest_path = PathBuf::from(dest);
    if !dest_path.is_absolute() {
        return Err("导出路径必须是绝对路径".to_string());
    }
    if dest_path.extension().and_then(|e| e.to_str()) != Some("tar") {
        return Err("导出文件必须以 .tar 结尾".to_string());
    }
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err("导出目录不存在".to_string());
        }
    }

    let mut items: Vec<(Value, PathBuf)> = Vec::new();
    for acc in &selected {
        let Some(id) = acc.get("id").and_then(Value::as_str) else {
            return Err("账号记录缺少 id".to_string());
        };
        let dir = snapshots_dir().join(id);
        if !dir.is_dir() {
            return Err(format!(
                "账号「{}」的快照目录缺失，无法导出",
                account_meta(acc)
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
            ));
        }
        items.push((acc.clone(), dir));
    }

    let size = write_bundle(&dest_path, &items)?;
    Ok(json!({
        "path": dest,
        "count": items.len(),
        "sizeBytes": size,
        "accounts": items.iter().map(|(a, _)| account_meta(a)).collect::<Vec<_>>(),
    }))
}

/// 打包：写 manifest + 递归追加快照目录。
fn write_bundle(dest: &Path, items: &[(Value, PathBuf)]) -> Result<u64, String> {
    let accounts: Vec<Value> = items.iter().map(|(a, _)| a.clone()).collect();
    let manifest = serde_json::to_vec_pretty(&json!({
        "format": BUNDLE_FORMAT,
        "version": BUNDLE_VERSION,
        "exportedAt": now_ms(),
        "accounts": accounts,
    }))
    .map_err(|e| e.to_string())?;

    let file = File::create(dest).map_err(|e| format!("创建导出文件失败：{e}"))?;
    let mut builder = tar::Builder::new(file);
    // UStar 头的数值字段必须显式给值（new_ustar 是零字节，tar 严格解析会拒绝）。
    let mut manifest_header = tar::Header::new_ustar();
    manifest_header.set_size(manifest.len() as u64);
    manifest_header.set_mode(0o644);
    manifest_header.set_mtime(0);
    builder
        .append_data(&mut manifest_header, MANIFEST_ENTRY, manifest.as_slice())
        .map_err(|e| format!("写入清单失败：{e}"))?;
    for (acc, dir) in items {
        let id = acc
            .get("id")
            .and_then(Value::as_str)
            .ok_or("账号记录缺少 id")?;
        builder
            .append_dir_all(format!("{ACCOUNTS_PREFIX}{id}"), dir)
            .map_err(|e| format!("写入快照 {id} 失败：{e}"))?;
    }
    let mut file = builder.into_inner().map_err(|e| format!("收尾导出文件失败：{e}"))?;
    file.flush().map_err(|e| e.to_string())?;
    file.metadata().map(|m| m.len()).map_err(|e| e.to_string())
}

/// 包内账号预览项。
fn preview_item(record: &Value, files: usize, current: &[Value]) -> Value {
    let id = record.get("id").and_then(Value::as_str);
    let uid = record.get("uid").and_then(Value::as_str);
    let client = record.get("client").and_then(Value::as_str);
    let conflict = if id.is_some() && current.iter().any(|a| a.get("id").and_then(Value::as_str) == id) {
        Some("sameId")
    } else if uid.is_some()
        && current.iter().any(|a| {
            a.get("uid").and_then(Value::as_str) == uid
                && a.get("client").and_then(Value::as_str) == client
        })
    {
        Some("sameAccount")
    } else {
        None
    };
    let mut meta = account_meta(record);
    meta["snapshotFiles"] = json!(files);
    meta["conflict"] = conflict.map(|c| json!(c)).unwrap_or(Value::Null);
    meta
}

/// 解析 tar 包：返回清单 + 每账号快照文件计数。不解压。
fn read_bundle(path: &str) -> Result<(Value, HashMap<String, usize>), String> {
    let file = File::open(path).map_err(|e| format!("打开迁移包失败：{e}"))?;
    let mut archive = tar::Archive::new(file);
    let mut manifest: Option<Value> = None;
    let mut counts: HashMap<String, usize> = HashMap::new();

    for entry in archive.entries().map_err(|e| format!("读取迁移包失败：{e}"))? {
        let mut entry = entry.map_err(|e| format!("读取包内条目失败：{e}"))?;
        let raw = entry.path().map_err(|e| format!("包内路径无效：{e}"))?;
        let rel = safe_relative(&raw)?;
        if rel == MANIFEST_ENTRY {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("读取清单失败：{e}"))?;
            manifest = Some(serde_json::from_str(&text).map_err(|e| format!("清单解析失败：{e}"))?);
        } else if let Some(rest) = rel.strip_prefix(ACCOUNTS_PREFIX) {
            // accounts/<id>/...：只统计常规文件，目录不计。
            if !rest.is_empty() && entry.header().entry_type().is_file() {
                if let Some(id) = rest.split('/').next() {
                    if !id.is_empty() {
                        *counts.entry(id.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let manifest = manifest.ok_or("迁移包缺少 export.json 清单")?;
    if manifest.get("format").and_then(Value::as_str) != Some(BUNDLE_FORMAT) {
        return Err("不是 TT Switch 账号迁移包".to_string());
    }
    if manifest.get("version").and_then(Value::as_u64) != Some(u64::from(BUNDLE_VERSION)) {
        return Err("迁移包版本不受支持".to_string());
    }
    Ok((manifest, counts))
}

/// 导入预览（含与本机账号库的冲突标记）。
pub fn preview_import(path: &str) -> Result<Value, String> {
    let (manifest, counts) = read_bundle(path)?;
    let current = load_accounts();
    let accounts: Vec<Value> = manifest
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("清单缺少 accounts")?
        .iter()
        .map(|a| {
            let id = a.get("id").and_then(Value::as_str).unwrap_or("");
            preview_item(a, *counts.get(id).unwrap_or(&0), &current)
        })
        .collect();
    Ok(json!({
        "path": path,
        "exportedAt": manifest.get("exportedAt").cloned().unwrap_or(Value::Null),
        "accounts": accounts,
    }))
}

/// 导入选中的账号（按预览下标）。同 client+uid 已存在则跳过；同 id 冲突换新 id。
pub fn import_accounts(path: &str, indexes: &[usize]) -> Result<Value, String> {
    let (manifest, _) = read_bundle(path)?;
    let bundle_accounts = manifest
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("清单缺少 accounts")?
        .clone();
    if bundle_accounts.is_empty() {
        return Err("迁移包内没有账号".to_string());
    }
    let wanted: Vec<&Value> = indexes
        .iter()
        .copied()
        .filter_map(|i| bundle_accounts.get(i))
        .collect();
    if wanted.is_empty() {
        return Err("未选择要导入的账号".to_string());
    }

    // 解压到存储根下的临时目录（逐条做路径穿越/类型校验）。
    let tmp = snapshots_dir().join(format!(".import-tmp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败：{e}"))?;
    if let Err(e) = extract_accounts(path, &wanted, &tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }

    let mut current = load_accounts();
    let root = snapshots_dir();
    let _ = std::fs::create_dir_all(&root);
    let (imported, skipped) = merge_into_library(&mut current, &tmp, &root, &wanted);
    let _ = std::fs::remove_dir_all(&tmp);
    if !imported.is_empty() {
        save_accounts(&current).map_err(|e| format!("保存账号库失败：{e}"))?;
    }
    Ok(json!({ "imported": imported, "skipped": skipped }))
}

/// 把临时目录中的快照并入账号库（纯逻辑，根目录显式传入，便于测试）。
/// 返回 (已导入元数据, 跳过项)。
fn merge_into_library(
    current: &mut Vec<Value>,
    tmp: &Path,
    snapshot_root: &Path,
    wanted: &[&Value],
) -> (Vec<Value>, Vec<Value>) {
    let mut imported: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    for acc in wanted {
        let Some(old_id) = acc.get("id").and_then(Value::as_str) else {
            skipped.push(json!({ "reason": "记录缺少 id" }));
            continue;
        };
        let uid = acc.get("uid").and_then(Value::as_str);
        let client = acc.get("client").and_then(Value::as_str);
        let name = account_meta(acc).get("displayName").cloned().unwrap_or(Value::Null);

        // 同 client+uid：本机已有该账号，跳过（避免重复快照）。
        if uid.is_some()
            && current.iter().any(|a| {
                a.get("uid").and_then(Value::as_str) == uid
                    && a.get("client").and_then(Value::as_str) == client
            })
        {
            skipped.push(json!({ "displayName": name, "reason": "sameAccount" }));
            continue;
        }

        // id 冲突（uid 不同）：分配新 id 并搬移快照目录。
        let new_id = if current
            .iter()
            .any(|a| a.get("id").and_then(Value::as_str) == Some(old_id))
        {
            uuid::Uuid::new_v4().to_string()
        } else {
            old_id.to_string()
        };
        let src = tmp.join(old_id);
        let dst = snapshot_root.join(&new_id);
        if !src.is_dir() {
            skipped.push(json!({ "displayName": name, "reason": "快照缺失" }));
            continue;
        }
        if dst.exists() {
            skipped.push(json!({ "displayName": name, "reason": "目标已存在" }));
            continue;
        }
        if let Err(e) = std::fs::rename(&src, &dst) {
            skipped.push(json!({ "displayName": name, "reason": format!("快照落盘失败: {e}") }));
            continue;
        }
        let mut record = (*acc).clone();
        record["id"] = json!(new_id);
        current.push(record.clone());
        imported.push(account_meta(&record));
    }
    (imported, skipped)
}

/// 仅解压选中账号到 `tmp/` 根（包内 accounts/<id> → tmp/<id>）。
fn extract_accounts(path: &str, wanted: &[&Value], tmp: &Path) -> Result<(), String> {
    let wanted_ids: Vec<&str> = wanted
        .iter()
        .filter_map(|a| a.get("id").and_then(Value::as_str))
        .collect();

    let file = File::open(path).map_err(|e| format!("打开迁移包失败：{e}"))?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries().map_err(|e| format!("读取迁移包失败：{e}"))? {
        let mut entry = entry.map_err(|e| format!("读取包内条目失败：{e}"))?;
        let raw = entry.path().map_err(|e| format!("包内路径无效：{e}"))?;
        let rel = safe_relative(&raw)?;
        let Some(rest) = rel.strip_prefix(ACCOUNTS_PREFIX) else {
            continue; // export.json 等
        };
        let Some(id) = rest.split('/').next() else { continue };
        if !wanted_ids.contains(&id) {
            continue;
        }
        // 不允许符号链接/特殊设备，只接受常规文件与目录。
        let et = entry.header().entry_type();
        if !(et.is_file() || et.is_dir()) {
            return Err(format!("包内存在不支持的条目类型：{rel}"));
        }
        let out = tmp.join(rest);
        if et.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("解压失败：{e}"))?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("解压失败：{e}"))?;
            }
            let mut writer = File::create(&out).map_err(|e| format!("解压失败：{e}"))?;
            std::io::copy(&mut entry, &mut writer).map_err(|e| format!("解压失败：{e}"))?;
        }
    }
    Ok(())
}

/// tar 内路径必须是相对路径、无 `..`、无盘符/根前缀；返回正斜杠相对串。
fn safe_relative(path: &Path) -> Result<String, String> {
    let mut parts: Vec<String> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(s) => parts.push(s.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("迁移包含非法路径（越界/绝对路径）".to_string());
            }
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fake_snapshot(root: &Path, id: &str, files: &[&str]) {
        let dir = root.join(id);
        for f in files {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, format!("blob-{id}-{f}")).unwrap();
        }
        std::fs::write(
            dir.join("manifest.json"),
            format!(r#"{{"id":"{id}","entries":[]}}"#),
        )
        .unwrap();
    }

    #[test]
    fn bundle_roundtrip_and_conflicts() {
        let work = std::env::temp_dir().join(format!("tt-switch-transfer-{}", uuid::Uuid::new_v4()));
        let src_root = work.join("snapshots");
        std::fs::create_dir_all(&src_root).unwrap();
        write_fake_snapshot(&src_root, "id-a", &["Cookies", "Local Storage/leveldb/001.log"]);
        write_fake_snapshot(&src_root, "id-b", &["Cookies"]);

        let dest = work.join("out.tar");
        let items = vec![
            (
                json!({"id":"id-a","uid":"u1","client":"ide_cn","nickname":"甲"}),
                src_root.join("id-a"),
            ),
            (
                json!({"id":"id-b","uid":"u2","client":"solo_cn"}),
                src_root.join("id-b"),
            ),
        ];
        let size = write_bundle(&dest, &items).unwrap();
        assert!(size > 0);

        // 空库预览：无冲突；文件数正确（Cookies、Local Storage 一个、manifest = 3）。
        let (manifest, counts) = read_bundle(dest.to_str().unwrap()).unwrap();
        assert_eq!(manifest["format"], BUNDLE_FORMAT);
        assert_eq!(counts.get("id-a"), Some(&3));
        assert_eq!(counts.get("id-b"), Some(&2));

        // 同 uid+client 判定 sameAccount；同 id 不同 uid 不判 sameAccount（留给导入换 id）。
        let current = vec![json!({"id":"x","uid":"u1","client":"ide_cn"})];
        let item = preview_item(&items[0].0, 3, &current);
        assert_eq!(item["conflict"], json!("sameAccount"));
        let current2 = vec![json!({"id":"id-a","uid":"u9","client":"ide_cn"})];
        let item2 = preview_item(&items[0].0, 3, &current2);
        assert_eq!(item2["conflict"], json!("sameId"));
    }

    #[test]
    fn safe_relative_rejects_traversal() {
        assert!(safe_relative(Path::new("accounts/x/../../etc")).is_err());
        assert!(safe_relative(Path::new("/etc/passwd")).is_err());
        assert_eq!(
            safe_relative(Path::new("accounts/x/y")).unwrap(),
            "accounts/x/y"
        );
    }

    #[test]
    fn import_extract_and_merge_roundtrip() {
        let work = std::env::temp_dir().join(format!("tt-switch-import-{}", uuid::Uuid::new_v4()));
        let src_root = work.join("src");
        let tmp = work.join("tmp");
        let final_root = work.join("store");
        std::fs::create_dir_all(&src_root).unwrap();
        std::fs::create_dir_all(&final_root).unwrap();
        write_fake_snapshot(&src_root, "id-a", &["Cookies"]);
        write_fake_snapshot(&src_root, "id-b", &["Cookies"]);

        let bundle = work.join("b.tar");
        let records = vec![
            json!({"id":"id-a","uid":"u1","client":"ide_cn","nickname":"甲"}),
            json!({"id":"id-b","uid":"u2","client":"solo_cn"}),
        ];
        let items: Vec<(Value, PathBuf)> = records
            .iter()
            .map(|r| (r.clone(), src_root.join(r["id"].as_str().unwrap())))
            .collect();
        write_bundle(&bundle, &items).unwrap();

        // 第一次：全部解压并入空库。
        let wanted: Vec<&Value> = records.iter().collect();
        extract_accounts(bundle.to_str().unwrap(), &wanted, &tmp).unwrap();
        let mut current: Vec<Value> = vec![];
        let (imported, skipped) = merge_into_library(&mut current, &tmp, &final_root, &wanted);
        assert_eq!(imported.len(), 2, "应导入 2 个: {skipped:?}");
        assert!(skipped.is_empty());
        assert!(current.len() == 2);
        assert!(final_root.join("id-a").join("Cookies").is_file());
        assert!(final_root.join("id-b").join("Cookies").is_file());

        // 第二次（同包再解压到新临时目录）：同 client+uid 全部跳过，不产生重复目录。
        let tmp2 = work.join("tmp2");
        extract_accounts(bundle.to_str().unwrap(), &wanted, &tmp2).unwrap();
        let (imported2, skipped2) = merge_into_library(&mut current, &tmp2, &final_root, &wanted);
        assert!(imported2.is_empty());
        assert_eq!(skipped2.len(), 2);
        assert!(current.len() == 2);
        let _ = std::fs::remove_dir_all(&work);
    }
}
