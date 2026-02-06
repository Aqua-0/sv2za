use crate::{backend::flatc, progress::ProgressSink};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub fn patch_personal_array_present(
    flatc_exe: &Path,
    za_dump: &Path,
    out_root: &Path,
    pknx_personal_dir: &Path,
    enable_keys: &HashSet<(u16, u16)>,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    progress.phase_start("Patch personal array");

    let personal_in = za_dump
        .join("avalon")
        .join("data")
        .join("personal_array.bin");
    if !personal_in.is_file() {
        progress.warn("[personal] personal_array.bin not found; skipping");
        progress.phase_end("Patch personal array");
        return Ok(());
    }
    let schema = pknx_personal_dir.join("PersonalTable.fbs");
    if !schema.is_file() {
        progress.warn(format!("[personal] missing schema: {:?}", schema));
        progress.phase_end("Patch personal array");
        return Ok(());
    }

    let td = tempfile::tempdir()?;
    let json_path = flatc::flatc_dump_json(
        flatc_exe,
        &schema,
        &[pknx_personal_dir.to_path_buf()],
        &personal_in,
        td.path(),
    )?;
    let mut doc: Value = serde_json::from_slice(&fs::read(&json_path)?)?;

    let table = doc
        .get_mut("Table")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("unexpected personal json shape: missing Table[]"))?;

    let mut missing = enable_keys.clone();
    let mut changed = 0usize;
    for e in table.iter_mut() {
        let Some(info) = e.get("Info").and_then(|v| v.as_object()) else {
            continue;
        };
        let sid = info
            .get("SpeciesInternal")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1) as i32;
        let form = info.get("Form").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        if sid < 0 || form < 0 {
            continue;
        }
        let k = (sid as u16, form as u16);
        if enable_keys.contains(&k) {
            if e.get("IsPresentInGame").and_then(|v| v.as_bool()) != Some(true) {
                if let Some(obj) = e.as_object_mut() {
                    obj.insert("IsPresentInGame".to_string(), Value::Bool(true));
                    changed += 1;
                }
            }
            missing.remove(&k);
        }
    }

    if !missing.is_empty() {
        let mut preview = missing.iter().take(20).copied().collect::<Vec<_>>();
        preview.sort();
        progress.warn(format!(
            "[personal] missing {} enable keys (first 20): {:?}",
            missing.len(),
            preview
        ));
    }

    let out_personal = out_root
        .join("avalon")
        .join("data")
        .join("personal_array.bin");
    if let Some(parent) = out_personal.parent() {
        fs::create_dir_all(parent)?;
    }
    if out_personal.is_file() {
        let bak = PathBuf::from(format!(
            "{}{}",
            out_personal.to_string_lossy(),
            ".pre_personal_patch.bak"
        ));
        if !bak.exists() {
            fs::copy(&out_personal, bak)?;
        }
    }

    let out_json = td.path().join("out.json");
    fs::write(&out_json, serde_json::to_vec_pretty(&doc)?)?;
    flatc::flatc_build_bin(
        flatc_exe,
        &schema,
        &[pknx_personal_dir.to_path_buf()],
        &out_json,
        &out_personal,
    )?;
    progress.info(format!(
        "[personal] enabled {} entries (requested {})",
        changed,
        enable_keys.len()
    ));
    progress.phase_end("Patch personal array");
    Ok(())
}
