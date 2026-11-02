//! Installing pet packs from a folder or a zip (design §5.1 "安装和导入"),
//! and removing user-installed packs.
//!
//! Every pack is validated before it is copied; an invalid pack is never
//! installed. A pack whose id already exists needs an explicit overwrite.

use crate::config::{self, DEFAULT_CODEX_PACK, DEFAULT_PACK};
use crate::pack;
use crate::server::Shared;
use serde::Serialize;
use serde_yaml::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Installed { id: String, name: String, replaced: bool },
    /// Same id already installed; ask before overwriting.
    Conflict { id: String, name: String, new_version: String, existing_version: String, existing_builtin: bool },
    Invalid { errors: Vec<String> },
}

fn staging(home: &Path) -> PathBuf {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    home.join("import-tmp").join(format!("{n:x}"))
}

/// Unpack a zip, refusing entries that would escape the target folder.
fn unzip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(zip_path).map_err(|e| crate::tr!("打不开 zip：{e}", "Can't open the zip: {e}"))?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| crate::tr!("不是有效的 zip：{e}", "Not a valid zip: {e}"))?;
    let mut total: u64 = 0;
    for i in 0..z.len() {
        let mut entry = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name() else { return Err(crate::tr!("zip 里有不安全的路径：{}", "Unsafe path in the zip: {}", entry.name())) };
        total += entry.size();
        if total > 200 * 1024 * 1024 {
            return Err(crate::tr!("zip 解压后超过 200 MB，不像是宠物包", "The zip unpacks to over 200 MB — that doesn't look like a pet pack"));
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut w = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The folder holding pet.json: the folder itself or its only subfolder
/// (zips usually wrap everything in one folder).
fn pack_root(dir: &Path) -> Option<PathBuf> {
    if dir.join("pet.json").is_file() {
        return Some(dir.to_path_buf());
    }
    let subs: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    match subs.as_slice() {
        [one] if one.join("pet.json").is_file() => Some(one.clone()),
        _ => None,
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t)?;
        } else {
            std::fs::copy(&p, &t)?;
        }
    }
    Ok(())
}

/// Install a pack from a folder or a .zip.
pub fn import(sh: &Shared, src: &Path, overwrite: bool) -> Result<Outcome, String> {
    let tmp = staging(&sh.paths.home);
    let result = import_inner(sh, src, overwrite, &tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

fn import_inner(sh: &Shared, src: &Path, overwrite: bool, tmp: &Path) -> Result<Outcome, String> {
    let is_zip = src.is_file() && src.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let base = if is_zip {
        std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
        unzip(src, tmp)?;
        tmp.to_path_buf()
    } else if src.is_dir() {
        src.to_path_buf()
    } else {
        return Err(crate::tr!("请选择宠物包文件夹或 .zip 文件", "Choose a pet pack folder or a .zip file"));
    };
    let Some(root) = pack_root(&base) else {
        return Ok(Outcome::Invalid { errors: vec![crate::tr!("找不到 pet.json（应该在文件夹根目录，或者唯一的子文件夹里）", "No pet.json found (it should be in the folder root, or in its only subfolder)")] });
    };
    let p = match pack::load(&root, false) {
        Ok(p) => p,
        Err(e) => return Ok(Outcome::Invalid { errors: e.errors }),
    };
    let dest = sh.paths.pets().join(&p.id);
    if std::fs::canonicalize(&root).ok() == std::fs::canonicalize(&dest).ok() {
        return Err(crate::tr!("这个宠物包已经在宠物目录里了", "This pet pack is already in the pets folder"));
    }
    let existing = sh.hub.lock().unwrap().lib.get(&p.id).cloned();
    if let (Some(old), false) = (&existing, overwrite) {
        return Ok(Outcome::Conflict {
            id: p.id.clone(),
            name: p.name.clone(),
            new_version: p.version.clone(),
            existing_version: old.version.clone(),
            existing_builtin: old.builtin,
        });
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| crate::tr!("删除旧版本失败：{e}", "Couldn't remove the old version: {e}"))?;
    }
    copy_dir(&root, &dest).map_err(|e| crate::tr!("复制失败：{e}", "Copy failed: {e}"))?;
    crate::rescan_packs(sh);
    Ok(Outcome::Installed { id: p.id, name: p.name, replaced: existing.is_some() })
}

/// Remove a user-installed pack. Camps, projects and rules that used it go
/// back to the defaults. Built-in packs cannot be removed (a user copy that
/// overrides one can; the built-in comes back).
pub fn remove(sh: &Shared, id: &str) -> Result<String, String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err(crate::tr!("无效的宠物包 id", "Invalid pet pack id"));
    }
    let dest = sh.paths.pets().join(id);
    if !dest.join("pet.json").is_file() {
        return Err(crate::tr!("只能删除自己导入的宠物包，内置宠物不能删除", "Only imported pet packs can be deleted; built-in pets can't"));
    }
    std::fs::remove_dir_all(&dest).map_err(|e| crate::tr!("删除失败：{e}", "Delete failed: {e}"))?;
    crate::rescan_packs(sh);
    let still_there = sh.hub.lock().unwrap().lib.get(id).is_some();
    if !still_there {
        // Nothing provides this id any more: point users of it back to defaults.
        let cfg = sh.hub.lock().unwrap().cfg.clone();
        let uses = cfg.selection.claude_code.as_deref() == Some(id)
            || cfg.selection.codex.as_deref() == Some(id)
            || cfg.selection.projects.values().any(|v| v == id)
            || cfg.pets.iter().any(|r| r.pack == id)
            || cfg.selection.favorites.iter().any(|f| f == id);
        if uses {
            config::edit(&sh.paths.config(), |m| {
                let sel = config::child(m, "selection");
                for (camp, def) in [("claude-code", DEFAULT_PACK), ("codex", DEFAULT_CODEX_PACK)] {
                    if sel.get(camp).and_then(Value::as_str) == Some(id) {
                        sel.insert(camp.into(), def.into());
                    }
                }
                if let Some(Value::Mapping(p)) = sel.get_mut("projects") {
                    p.retain(|_, v| v.as_str() != Some(id));
                }
                if let Some(Value::Sequence(f)) = sel.get_mut("favorites") {
                    f.retain(|v| v.as_str() != Some(id));
                }
                if let Some(Value::Sequence(rules)) = m.get_mut("pets") {
                    for r in rules.iter_mut() {
                        if r.get("pack").and_then(Value::as_str) == Some(id) {
                            if let Some(rm) = r.as_mapping_mut() {
                                rm.insert("pack".into(), DEFAULT_PACK.into());
                            }
                        }
                    }
                }
                Ok(())
            })?;
            crate::ops::reload_config(sh);
        }
    }
    Ok(if still_there {
        crate::tr!("已删除导入的 {id}，恢复为内置版本。", "Deleted the imported {id}; the built-in version is back.")
    } else {
        crate::tr!("已删除宠物包 {id}。", "Deleted pet pack {id}.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::hub::Hub;
    use crate::paths::Paths;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    fn shared(home: &Path, builtin: &Path) -> Shared {
        let paths = Paths::at(home);
        paths.ensure().unwrap();
        std::fs::write(paths.config(), config::DEFAULT_YAML).unwrap();
        let roots = vec![(builtin.to_path_buf(), true), (paths.pets(), false)];
        let lib = pack::Library::scan(&roots);
        let (ws_tx, _) = tokio::sync::broadcast::channel(64);
        let (control, _) = tokio::sync::broadcast::channel(64);
        Shared {
            hub: Arc::new(Mutex::new(Hub::new(Config::load(&paths.config()).unwrap(), lib))),
            token: "t".into(),
            paths,
            ws_tx,
            control,
            urgent: Arc::new(tokio::sync::Notify::new()),
            roots,
            world: Arc::new(Mutex::new(crate::world::World::new(crate::balance::Tables::builtin(), 1))),
            db: Arc::new(crate::db::Db::in_memory()),
        }
    }

    /// Enough of a PNG for the IHDR size check.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    /// A minimal pack: one 8x8 idle strip that really holds 2 frames.
    fn make_pack(dir: &Path, id: &str, version: &str, frames_declared: u32) {
        std::fs::create_dir_all(dir.join("sprites")).unwrap();
        std::fs::write(dir.join("sprites/idle.png"), png(16, 8)).unwrap();
        let json = serde_json::json!({
            "id": id, "name": "测试", "version": version, "frame": { "w": 8, "h": 8 },
            "actions": { "idle": { "sheet": "sprites/idle.png", "frames": frames_declared, "fps": 2, "loop": true } }
        });
        std::fs::write(dir.join("pet.json"), json.to_string()).unwrap();
    }

    fn setup() -> (tempfile::TempDir, Shared) {
        let tmp = tempfile::tempdir().unwrap();
        let builtin = tmp.path().join("builtin");
        std::fs::create_dir_all(&builtin).unwrap();
        let sh = shared(&tmp.path().join("home"), &builtin);
        (tmp, sh)
    }

    #[test]
    fn folder_import_conflict_overwrite_and_invalid() {
        let (tmp, sh) = setup();
        let src = tmp.path().join("src/dog");
        make_pack(&src, "dog", "1.0.0", 2);

        let out = import(&sh, &src, false).unwrap();
        assert_eq!(out, Outcome::Installed { id: "dog".into(), name: "测试".into(), replaced: false });
        assert!(sh.hub.lock().unwrap().lib.get("dog").is_some(), "visible right away");

        make_pack(&src, "dog", "1.1.0", 2);
        match import(&sh, &src, false).unwrap() {
            Outcome::Conflict { existing_version, new_version, existing_builtin, .. } => {
                assert_eq!((existing_version.as_str(), new_version.as_str(), existing_builtin), ("1.0.0", "1.1.0", false));
            }
            o => panic!("{o:?}"),
        }
        assert!(matches!(import(&sh, &src, true).unwrap(), Outcome::Installed { replaced: true, .. }));
        assert_eq!(sh.hub.lock().unwrap().lib.get("dog").unwrap().version, "1.1.0");

        let bad = tmp.path().join("src/bad");
        make_pack(&bad, "bad", "1", 3); // the strip holds 2 frames, pet.json says 3
        match import(&sh, &bad, false).unwrap() {
            Outcome::Invalid { errors } => assert!(errors[0].contains("应为"), "{errors:?}"),
            o => panic!("{o:?}"),
        }
        assert!(!sh.paths.pets().join("bad").exists(), "invalid packs are never installed");
    }

    #[test]
    fn zip_import_and_zip_slip() {
        let (tmp, sh) = setup();
        let src = tmp.path().join("src/cat2");
        make_pack(&src, "cat2", "1", 2);

        // Wrapped in one folder, the way zips usually are.
        let zpath = tmp.path().join("cat2.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zpath).unwrap());
            let opt = zip::write::SimpleFileOptions::default();
            for (name, file) in [("cat2/pet.json", src.join("pet.json")), ("cat2/sprites/idle.png", src.join("sprites/idle.png"))] {
                z.start_file(name, opt).unwrap();
                z.write_all(&std::fs::read(file).unwrap()).unwrap();
            }
            z.finish().unwrap();
        }
        assert!(matches!(import(&sh, &zpath, false).unwrap(), Outcome::Installed { .. }));
        assert!(sh.paths.pets().join("cat2/sprites/idle.png").is_file());

        let evil = tmp.path().join("evil.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&evil).unwrap());
            z.start_file("../../escaped.txt", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"x").unwrap();
            z.finish().unwrap();
        }
        assert!(import(&sh, &evil, false).is_err());
        assert!(!tmp.path().join("escaped.txt").exists());
    }

    #[test]
    fn removing_a_pack_in_use_falls_back_to_defaults() {
        let (tmp, sh) = setup();
        let src = tmp.path().join("src/dog");
        make_pack(&src, "dog", "1", 2);
        import(&sh, &src, false).unwrap();
        crate::ops::select(&sh, "codex", "dog").unwrap();
        crate::ops::set_favorite(&sh, "dog", true).unwrap();
        assert_eq!(sh.hub.lock().unwrap().cfg.selection.codex.as_deref(), Some("dog"));

        remove(&sh, "dog").unwrap();
        let cfg = sh.hub.lock().unwrap().cfg.clone();
        assert_eq!(cfg.selection.codex.as_deref(), Some(DEFAULT_CODEX_PACK));
        assert!(!cfg.selection.favorites.contains(&"dog".to_string()));
        assert!(remove(&sh, "pixel-cat").is_err(), "built-ins are not removable");
    }
}
