#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, fs, path::{Path, PathBuf}, process::Command, sync::atomic::{AtomicU64, Ordering}, time::{SystemTime, UNIX_EPOCH}};

static ITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: String,
    category: String,
    title: String,
    description: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    open_count: u64,
    #[serde(default)]
    last_opened_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct Catalog { version: u8, items: Vec<Item> }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewItem { category: String, title: String, description: String, tags: Vec<String>, extension: String, content: String, url: String, #[serde(default)] icon: Option<String> }

fn workspace_root() -> PathBuf {
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() { if parent.join("data").is_dir() { return parent.to_path_buf(); } }
    }
    if let Ok(cwd) = env::current_dir() {
        for candidate in cwd.ancestors() { if candidate.join("data").is_dir() { return candidate.to_path_buf(); } }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn data_root() -> PathBuf { workspace_root().join("data") }
fn catalog_path() -> PathBuf { data_root().join("catalog.json") }
fn now() -> String { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs().to_string() }
fn new_id() -> String { format!("{}-{}-{}", now(), std::process::id(), ITEM_COUNTER.fetch_add(1, Ordering::Relaxed)) }

fn ensure_directories() -> Result<(), String> {
    for name in ["脚本", "网址", "待整理", "密码库", "备份"] { fs::create_dir_all(data_root().join(name)).map_err(|e| e.to_string())?; }
    Ok(())
}

fn read_catalog() -> Catalog {
    fs::read_to_string(catalog_path()).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Catalog { version: 1, items: vec![] })
}

fn write_catalog(catalog: &Catalog) -> Result<(), String> {
    let path = catalog_path(); let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

fn clean_file_name(title: &str) -> String {
    let name: String = title.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '-' } else { c }).collect();
    let trimmed = name.trim_matches('.').trim(); if trimmed.is_empty() { "untitled".to_string() } else { trimmed.chars().take(80).collect() }
}

fn source_join(source: &str) -> Option<PathBuf> {
    let candidate = Path::new(source); if candidate.components().count() != 2 { return None; }
    let full = data_root().join(candidate); if full.starts_with(data_root()) { Some(full) } else { None }
}

fn scan(catalog: &mut Catalog) -> Result<(Vec<Item>, Vec<Item>), String> {
    ensure_directories()?; let mut discovered = vec![]; let mut removed = vec![];
    catalog.items.retain(|item| {
        let Some(source) = item.source_path.as_deref() else { return true; };
        let Some(path) = source_join(source) else { return true; };
        if path.is_file() { true } else { removed.push(item.clone()); false }
    });
    for (folder, category) in [("脚本", "script"), ("网址", "link"), ("待整理", "inbox")] {
        for entry in fs::read_dir(data_root().join(folder)).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?; if !entry.file_type().map_err(|e| e.to_string())?.is_file() { continue; }
            let file_name = entry.file_name().to_string_lossy().to_string(); if file_name == ".gitkeep" { continue; }
            let source_path = format!("{}/{}", folder, file_name);
            if catalog.items.iter().any(|item| item.source_path.as_deref() == Some(&source_path)) { continue; }
            let stamp = now(); let mut item = Item { id: new_id(), category: category.into(), title: Path::new(&file_name).file_stem().unwrap_or_default().to_string_lossy().to_string(), description: String::new(), tags: vec![], source_path: Some(source_path), url: None, status: "needs_metadata".into(), created_at: stamp.clone(), updated_at: stamp, open_count: 0, last_opened_at: String::new(), icon: None, order: None };
            if category == "link" && file_name.to_lowercase().ends_with(".url") { item.url = fs::read_to_string(entry.path()).ok().and_then(|s| s.lines().find_map(|line| line.strip_prefix("URL=").map(str::to_string))); }
            catalog.items.push(item.clone()); discovered.push(item);
        }
    }
    write_catalog(catalog)?; Ok((discovered, removed))
}

#[tauri::command]
fn get_status() -> Value { json!({ "ok": true, "dataPath": data_root().to_string_lossy() }) }
#[tauri::command]
fn get_items() -> Value { json!({ "items": read_catalog().items }) }
#[tauri::command]
fn scan_items() -> Result<Value, String> { let mut catalog = read_catalog(); let (discovered, removed) = scan(&mut catalog)?; Ok(json!({ "discovered": discovered, "removed": removed, "items": catalog.items })) }

#[tauri::command]
fn create_item(input: NewItem) -> Result<Value, String> {
    ensure_directories()?; if input.title.trim().is_empty() { return Err("类别和名称不能为空。".into()); }
    let mut catalog = read_catalog(); let stamp = now(); let mut item = Item { id: new_id(), category: input.category.clone(), title: input.title.trim().into(), description: input.description.trim().into(), tags: input.tags.into_iter().filter(|x| !x.is_empty()).take(20).collect(), source_path: None, url: None, status: "complete".into(), created_at: stamp.clone(), updated_at: stamp, open_count: 0, last_opened_at: String::new(), icon: input.icon.filter(|x| x.len() <= 2 * 1024 * 1024), order: None };
    if input.category == "script" { let ext: String = input.extension.chars().filter(|c| c.is_ascii_alphanumeric()).take(10).collect(); let file = format!("{}.{}", clean_file_name(&item.title), if ext.is_empty() { "txt" } else { &ext }); item.source_path = Some(format!("脚本/{}", file)); fs::write(data_root().join("脚本").join(file), input.content).map_err(|e| e.to_string())?; }
    else if input.category == "link" { if !(input.url.starts_with("https://") || input.url.starts_with("http://")) { return Err("请输入有效的 http 或 https 网址。".into()); } item.url = Some(input.url); }
    else { return Err("不支持的类别。".into()); }
    catalog.items.push(item.clone()); write_catalog(&catalog)?; Ok(json!({ "item": item }))
}

#[tauri::command]
fn complete_metadata(input: NewItemWithId) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let item = catalog.items.iter_mut().find(|x| x.id == input.id).ok_or("找不到待处理条目。")?;
    if input.title.trim().is_empty() { return Err("名称不能为空。".into()); }
    if item.category == "inbox" {
        let old = item.source_path.as_deref().and_then(source_join).ok_or("文件路径无效。")?;
        let file_name = old.file_name().ok_or("文件名无效。")?.to_string_lossy().to_string();
        let folder = if input.category == "link" {
            if old.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("url")) != Some(true) { return Err("网址分类仅支持 .url 快捷方式文件。".into()); }
            "网址"
        } else { "脚本" };
        let target = data_root().join(folder).join(&file_name);
        if old.exists() && !target.exists() { fs::rename(&old, &target).map_err(|e| e.to_string())?; }
        item.source_path = Some(format!("{}/{}", folder, file_name));
    }
    item.category = if input.category == "link" { "link" } else { "script" }.into(); item.title = input.title.trim().into(); item.description = input.description.trim().into(); item.tags = input.tags.into_iter().filter(|x| !x.is_empty()).take(20).collect(); item.status = "complete".into(); item.updated_at = now(); let result = item.clone(); write_catalog(&catalog)?; Ok(json!({ "item": result }))
}

#[derive(Deserialize)]
struct HideItem { id: String }

#[tauri::command]
fn hide_item(input: HideItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let item = catalog.items.iter_mut().find(|x| x.id == input.id && x.status == "needs_metadata").ok_or("找不到待补充条目。")?;
    item.status = "hidden".into();
    item.updated_at = now();
    let result = item.clone();
    write_catalog(&catalog)?;
    Ok(json!({ "item": result }))
}

#[derive(Deserialize)] #[serde(rename_all = "camelCase")]
struct NewItemWithId { id: String, category: String, title: String, description: String, tags: Vec<String> }

#[derive(Deserialize)]
struct OpenItem { id: String }

#[tauri::command]
fn open_item(input: OpenItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let item = catalog.items.iter_mut().find(|x| x.id == input.id).ok_or("找不到该条目。")?;
    if item.status != "complete" { return Err("请先补充该条目的分类信息。".into()); }
    if item.category == "script" {
        let target = item.source_path.as_deref().and_then(source_join).ok_or("脚本路径无效。")?;
        if !target.is_file() { return Err("找不到脚本文件。请先扫描并同步。".into()); }
        Command::new("cmd").arg("/C").arg("start").arg("").arg(target).spawn().map_err(|e| format!("无法启动脚本：{}", e))?;
    } else if item.category == "link" {
        let url = item.url.as_deref().ok_or("网址无效。")?;
        if !(url.starts_with("https://") || url.starts_with("http://")) { return Err("网址仅支持 http 或 https。".into()); }
        Command::new("cmd").arg("/C").arg("start").arg("").arg(url).spawn().map_err(|e| format!("无法打开网址：{}", e))?;
    } else { return Err("该类别暂不支持打开。".into()); }
    item.open_count += 1; item.last_opened_at = now(); item.updated_at = now();
    write_catalog(&catalog)?; Ok(json!({ "ok": true }))
}

#[derive(Deserialize)] #[serde(rename_all = "camelCase")]
struct UpdateItem { id: String, title: String, description: String, tags: Vec<String>, #[serde(default)] icon: Option<String> }
#[tauri::command]
fn update_item(input: UpdateItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let item = catalog.items.iter_mut().find(|x| x.id == input.id).ok_or("找不到条目。")?;
    if input.title.trim().is_empty() { return Err("名称不能为空。".into()); }
    item.title = input.title.trim().into(); item.description = input.description.trim().into(); item.tags = input.tags.into_iter().filter(|x| !x.is_empty()).take(20).collect();
    if let Some(icon) = input.icon { item.icon = if icon.len() <= 2 * 1024 * 1024 { Some(icon) } else { return Err("图片文件过大，请选择 1.5MB 以下的图片。".into()) }; }
    item.updated_at = now(); let result = item.clone(); write_catalog(&catalog)?; Ok(json!({ "item": result }))
}
#[derive(Deserialize)] struct DeleteItem { id: String }
#[tauri::command]
fn delete_item(input: DeleteItem) -> Result<Value, String> {
    let mut catalog = read_catalog(); let index = catalog.items.iter().position(|x| x.id == input.id).ok_or("找不到条目。")?;
    let item = catalog.items.remove(index); if let Some(path) = item.source_path.as_deref().and_then(source_join) { if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; } }
    write_catalog(&catalog)?; Ok(json!({ "ok": true }))
}
#[derive(Deserialize)] struct ReorderItems { ids: Vec<String> }
#[tauri::command]
fn reorder_items(input: ReorderItems) -> Result<Value, String> {
    let mut catalog = read_catalog(); for item in &mut catalog.items { if let Some(position) = input.ids.iter().position(|id| id == &item.id) { item.order = Some(position as u64); } } write_catalog(&catalog)?; Ok(json!({ "items": catalog.items }))
}

fn main() {
    tauri::Builder::default().invoke_handler(tauri::generate_handler![get_status, get_items, scan_items, create_item, complete_metadata, hide_item, open_item, update_item, delete_item, reorder_items]).run(tauri::generate_context!()).expect("启动客户端失败");
}
