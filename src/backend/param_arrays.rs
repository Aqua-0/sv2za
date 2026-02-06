use crate::{backend::flatc, progress::ProgressSink};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub fn patch_param_arrays(
    flatc_exe: &Path,
    za_dump: &Path,
    out_root: &Path,
    donor_dev: u32,
    new_species: &HashSet<u16>,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    progress.phase_start("Patch param arrays");

    let model_bin_in = za_dump
        .join("param_chr")
        .join("data")
        .join("pokemon")
        .join("poke_model_param")
        .join("poke_model_param_array.bin");
    let model_bfbs = za_dump
        .join("param_chr")
        .join("data")
        .join("pokemon")
        .join("poke_model_param")
        .join("poke_model_param_array.bfbs");
    let move_bin_in = za_dump
        .join("param_chr")
        .join("data")
        .join("character")
        .join("pokemon")
        .join("poke_movement_param")
        .join("poke_movement_param_array.bin");
    let move_bfbs = za_dump
        .join("param_chr")
        .join("data")
        .join("character")
        .join("pokemon")
        .join("poke_movement_param")
        .join("poke_movement_param_array.bfbs");

    if !model_bin_in.is_file() || !model_bfbs.is_file() {
        progress.warn("[param] missing ZA model param bin/bfbs; skipping");
    } else {
        let model_out = out_root
            .join("param_chr")
            .join("data")
            .join("pokemon")
            .join("poke_model_param")
            .join("poke_model_param_array.bin");
        patch_one(
            flatc_exe,
            &model_bfbs,
            &model_bin_in,
            &model_out,
            "devId",
            donor_dev,
            new_species,
            progress,
        )?;
    }

    if !move_bin_in.is_file() || !move_bfbs.is_file() {
        progress.warn("[param] missing ZA movement param bin/bfbs; skipping");
    } else {
        let move_out = out_root
            .join("param_chr")
            .join("data")
            .join("character")
            .join("pokemon")
            .join("poke_movement_param")
            .join("poke_movement_param_array.bin");
        patch_one(
            flatc_exe,
            &move_bfbs,
            &move_bin_in,
            &move_out,
            "devNo",
            donor_dev,
            new_species,
            progress,
        )?;
    }

    progress.phase_end("Patch param arrays");
    Ok(())
}

pub fn patch_param_arrays_per_species(
    flatc_exe: &Path,
    za_dump: &Path,
    out_root: &Path,
    donor_by_species: &std::collections::BTreeMap<u16, u16>,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    progress.phase_start("Patch param arrays");

    let model_bin_in = za_dump
        .join("param_chr")
        .join("data")
        .join("pokemon")
        .join("poke_model_param")
        .join("poke_model_param_array.bin");
    let model_bfbs = za_dump
        .join("param_chr")
        .join("data")
        .join("pokemon")
        .join("poke_model_param")
        .join("poke_model_param_array.bfbs");
    let move_bin_in = za_dump
        .join("param_chr")
        .join("data")
        .join("character")
        .join("pokemon")
        .join("poke_movement_param")
        .join("poke_movement_param_array.bin");
    let move_bfbs = za_dump
        .join("param_chr")
        .join("data")
        .join("character")
        .join("pokemon")
        .join("poke_movement_param")
        .join("poke_movement_param_array.bfbs");

    if model_bin_in.is_file() && model_bfbs.is_file() {
        let model_out = out_root
            .join("param_chr")
            .join("data")
            .join("pokemon")
            .join("poke_model_param")
            .join("poke_model_param_array.bin");
        patch_one_with_map(
            flatc_exe,
            &model_bfbs,
            &model_bin_in,
            &model_out,
            "devId",
            donor_by_species,
            progress,
        )?;
    } else {
        progress.warn("[param] missing ZA model param bin/bfbs; skipping");
    }

    if move_bin_in.is_file() && move_bfbs.is_file() {
        let move_out = out_root
            .join("param_chr")
            .join("data")
            .join("character")
            .join("pokemon")
            .join("poke_movement_param")
            .join("poke_movement_param_array.bin");
        patch_one_with_map(
            flatc_exe,
            &move_bfbs,
            &move_bin_in,
            &move_out,
            "devNo",
            donor_by_species,
            progress,
        )?;
    } else {
        progress.warn("[param] missing ZA movement param bin/bfbs; skipping");
    }

    progress.phase_end("Patch param arrays");
    Ok(())
}

fn patch_one(
    flatc_exe: &Path,
    bfbs: &Path,
    src_bin: &Path,
    out_bin: &Path,
    key: &str,
    donor_dev: u32,
    new_species: &HashSet<u16>,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    if let Some(parent) = out_bin.parent() {
        fs::create_dir_all(parent)?;
    }
    if out_bin.is_file() {
        let bak = PathBuf::from(format!(
            "{}{}",
            out_bin.to_string_lossy(),
            ".pre_param_patch.bak"
        ));
        if !bak.exists() {
            fs::copy(out_bin, bak)?;
        }
    }

    let td = tempfile::tempdir()?;
    let json_path = flatc::flatc_dump_json(flatc_exe, bfbs, &[], src_bin, td.path())?;
    let mut obj: Value = serde_json::from_slice(&fs::read(&json_path)?)?;

    let values = obj
        .get_mut("values")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("unexpected param json shape: missing values[]"))?;

    let mut existing = HashSet::<u16>::new();
    for item in values.iter() {
        if let Some(v) = extract_single_root_entry(item).and_then(|e| e.get(key)) {
            if let Some(n) = v.as_u64() {
                existing.insert(n as u16);
            }
        }
    }

    let mut needed = new_species
        .iter()
        .copied()
        .filter(|s| !existing.contains(s))
        .collect::<Vec<_>>();
    needed.sort();
    if needed.is_empty() {
        progress.info(format!(
            "[param] {} already contains all ids ({}); no change",
            out_bin.file_name().unwrap_or_default().to_string_lossy(),
            key
        ));
        if !out_bin.exists() {
            fs::copy(src_bin, out_bin)?;
        }
        return Ok(());
    }

    let donor_idx = find_index_by_key(values, key, donor_dev as u64)
        .ok_or_else(|| anyhow::anyhow!("donor not found: {key}={donor_dev}"))?;
    let donor_entry = extract_single_root_entry(&values[donor_idx])
        .ok_or_else(|| anyhow::anyhow!("unexpected donor entry shape"))?
        .clone();

    let mut added = 0usize;
    for new_id in needed {
        let mut new_entry = donor_entry.clone();
        if let Some(obj) = new_entry.as_object_mut() {
            obj.insert(key.to_string(), Value::from(new_id as u64));
        }
        let new_item = Value::Object(serde_json::Map::from_iter([(
            "root".to_string(),
            Value::Array(vec![new_entry]),
        )]));
        insert_sorted_by_key(values, key, &new_item)?;
        added += 1;
    }

    let out_json = td.path().join("out.json");
    fs::write(&out_json, serde_json::to_vec_pretty(&obj)?)?;
    flatc::flatc_build_bin(flatc_exe, bfbs, &[], &out_json, out_bin)?;
    progress.info(format!(
        "[param] patched {}: added {} ({}) from donor {}",
        out_bin.file_name().unwrap_or_default().to_string_lossy(),
        added,
        key,
        donor_dev
    ));
    Ok(())
}

fn patch_one_with_map(
    flatc_exe: &Path,
    bfbs: &Path,
    src_bin: &Path,
    out_bin: &Path,
    key: &str,
    donor_by_species: &std::collections::BTreeMap<u16, u16>,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    if let Some(parent) = out_bin.parent() {
        fs::create_dir_all(parent)?;
    }

    let td = tempfile::tempdir()?;
    let json_path = flatc::flatc_dump_json(flatc_exe, bfbs, &[], src_bin, td.path())?;
    let mut obj: Value = serde_json::from_slice(&fs::read(&json_path)?)?;

    let values = obj
        .get_mut("values")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| anyhow::anyhow!("unexpected param json shape: missing values[]"))?;

    let mut existing = HashSet::<u16>::new();
    for item in values.iter() {
        if let Some(v) = extract_single_root_entry(item).and_then(|e| e.get(key)) {
            if let Some(n) = v.as_u64() {
                existing.insert(n as u16);
            }
        }
    }

    let mut donor_entry_by_id = std::collections::HashMap::<u16, Value>::new();
    for &donor_id in donor_by_species.values() {
        if donor_entry_by_id.contains_key(&donor_id) {
            continue;
        }
        if let Some(idx) = find_index_by_key(values, key, donor_id as u64) {
            if let Some(e) = extract_single_root_entry(&values[idx]) {
                donor_entry_by_id.insert(donor_id, e.clone());
            }
        }
    }

    let mut added = 0usize;
    for (&target_id, &donor_id) in donor_by_species {
        if existing.contains(&target_id) {
            continue;
        }
        let Some(donor_entry) = donor_entry_by_id.get(&donor_id) else {
            progress.warn(format!("[param] donor not found: {key}={donor_id}"));
            continue;
        };
        let mut new_entry = donor_entry.clone();
        if let Some(obj) = new_entry.as_object_mut() {
            obj.insert(key.to_string(), Value::from(target_id as u64));
        }
        let new_item = Value::Object(serde_json::Map::from_iter([(
            "root".to_string(),
            Value::Array(vec![new_entry]),
        )]));
        insert_sorted_by_key(values, key, &new_item)?;
        added += 1;
    }

    if added == 0 {
        if !out_bin.exists() {
            fs::copy(src_bin, out_bin)?;
        }
        return Ok(());
    }

    let out_json = td.path().join("out.json");
    fs::write(&out_json, serde_json::to_vec_pretty(&obj)?)?;
    flatc::flatc_build_bin(flatc_exe, bfbs, &[], &out_json, out_bin)?;
    progress.info(format!(
        "[param] patched {}: added {} ({})",
        out_bin.file_name().unwrap_or_default().to_string_lossy(),
        added,
        key
    ));
    Ok(())
}

fn extract_single_root_entry(item: &Value) -> Option<&Value> {
    let root = item.get("root")?.as_array()?;
    if root.len() != 1 {
        return None;
    }
    let e = &root[0];
    if !e.is_object() {
        return None;
    }
    Some(e)
}

fn find_index_by_key(values: &[Value], key: &str, value: u64) -> Option<usize> {
    for (i, item) in values.iter().enumerate() {
        let e = extract_single_root_entry(item)?;
        if e.get(key).and_then(|v| v.as_u64()) == Some(value) {
            return Some(i);
        }
    }
    None
}

fn insert_sorted_by_key(
    values: &mut Vec<Value>,
    key: &str,
    new_item: &Value,
) -> anyhow::Result<()> {
    let new_e = extract_single_root_entry(new_item)
        .ok_or_else(|| anyhow::anyhow!("new item has unexpected shape"))?;
    let new_id = new_e
        .get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("new entry missing int {key}"))?;

    let mut insert_at = values.len();
    for (i, item) in values.iter().enumerate() {
        let Some(e) = extract_single_root_entry(item) else {
            continue;
        };
        let Some(cur) = e.get(key).and_then(|v| v.as_u64()) else {
            continue;
        };
        if cur > new_id {
            insert_at = i;
            break;
        }
    }
    values.insert(insert_at, new_item.clone());
    Ok(())
}
