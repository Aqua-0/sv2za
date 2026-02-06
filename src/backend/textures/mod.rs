mod bmp;
mod bntx;
mod index;

use crate::{config::AppConfig, progress::ProgressSink};
use bntx::{extract_tex_data, read_bntx_metas, ultimate_format, BntxIndexDoc, BntxIndexEntry};
use index::{default_cache_path, load_or_build_index};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};
use walkdir::WalkDir;

pub fn convert_textures_if_enabled(
    cfg: &AppConfig,
    za_dump: &Path,
    out_root: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    if !cfg.texture_convert {
        return Ok(());
    }
    let ultimate = cfg
        .ultimate_tex_cli
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("texture_convert enabled but ultimate_tex_cli not set"))?;
    if !ultimate.is_file() {
        anyhow::bail!("ultimate_tex_cli not found: {ultimate:?}");
    }

    let cache_path = default_cache_path(out_root);
    let index = load_or_build_index(za_dump, &cache_path, progress)?;
    convert_dir(
        ultimate,
        &index,
        &out_root.join("ik_pokemon").join("data"),
        cfg.texture_allow_resize,
        progress,
    )
}

fn convert_dir(
    ultimate: &Path,
    index: &BntxIndexDoc,
    input_dir: &Path,
    allow_resize: bool,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    progress.phase_start("Texture convert");
    if !input_dir.is_dir() {
        progress.warn(format!("[tex] missing dir: {:?}", input_dir));
        progress.phase_end("Texture convert");
        return Ok(());
    }

    let entries = &index.entries;
    let default_icon = select_default_icon_donor(entries);
    let by_key = &index.by_key;
    let by_name = &index.by_name;

    let mut files = Vec::new();
    for e in WalkDir::new(input_dir).follow_links(false) {
        let e = e?;
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().extension().and_then(|x| x.to_str()) == Some("bntx") {
            files.push(e.path().to_path_buf());
        }
    }
    files.sort();
    let total = files.len().max(1) as u64;
    let mut done = 0u64;
    let mut ok = 0u64;
    let mut skipped = 0u64;
    let mut failed = 0u64;
    let start = Instant::now();

    for src in files {
        done += 1;
        progress.progress(done, total);
        if done % 100 == 0 || done == total {
            let secs = start.elapsed().as_secs_f64().max(0.001);
            let rate = (done as f64) / secs;
            let rem = (total - done) as f64;
            let eta_s = if rate > 0.0 { rem / rate } else { 0.0 };
            progress.info(format!("[tex] {done}/{total} ETA~{eta_s:.0}s"));
        }

        let metas = match read_bntx_metas(&src) {
            Ok(m) => m,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let Some(m0) = metas.first() else {
            skipped += 1;
            continue;
        };

        let donor = pick_donor(&src, m0, entries, by_key, by_name, &default_icon);
        let Some(donor) = donor else {
            skipped += 1;
            continue;
        };
        if already_converted(m0, donor) {
            skipped += 1;
            continue;
        }

        match convert_one(&src, &src, donor, ultimate, allow_resize, progress) {
            Ok(true) => ok += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                failed += 1;
                progress.warn(format!(
                    "[tex] failed {:?}: {e}",
                    src.file_name().unwrap_or_default()
                ));
            }
        }
    }

    progress.info(format!("[tex] ok={ok} skipped={skipped} failed={failed}"));
    progress.phase_end("Texture convert");
    Ok(())
}

fn select_default_icon_donor(entries: &[BntxIndexEntry]) -> Option<BntxIndexEntry> {
    let mut counts = HashMap::<(i32, i32, String, bool), u32>::new();
    for e in entries {
        let p = e.file_path.replace('\\', "/").to_lowercase();
        if !p.contains("/ik_pokemon/data/") || !p.contains("/icon/") {
            continue;
        }
        let Some(fmt) = e.ultimate_format.clone() else {
            continue;
        };
        let k = (e.width, e.height, fmt, e.no_mipmaps);
        *counts.entry(k).or_default() += 1;
    }
    let (best, _) = counts.into_iter().max_by_key(|(_, c)| *c)?;
    for e in entries {
        if e.width == best.0
            && e.height == best.1
            && e.ultimate_format.as_deref() == Some(best.2.as_str())
            && e.no_mipmaps == best.3
        {
            return Some(e.clone());
        }
    }
    None
}

fn pick_donor<'a>(
    src_path: &Path,
    meta: &bntx::BntxMeta,
    entries: &'a [BntxIndexEntry],
    by_key: &HashMap<String, Vec<usize>>,
    by_name: &HashMap<String, Vec<usize>>,
    default_icon: &'a Option<BntxIndexEntry>,
) -> Option<&'a BntxIndexEntry> {
    let src_ult = ultimate_format(meta.format_type, meta.format_var).map(|s| s.to_string());
    let src_no_mip = meta.mip_count <= 1;
    if let Some(src_ult) = src_ult {
        let k = format!(
            "{}x{}|{}|noMip={}",
            meta.width,
            meta.height,
            src_ult,
            if src_no_mip { 1 } else { 0 }
        );
        if let Some(idxs) = by_key.get(&k) {
            for &i in idxs {
                if let Some(d) = entries.get(i) {
                    if d.ultimate_format.is_some() {
                        return Some(d);
                    }
                }
            }
        }
    }

    let name_l = src_path.file_name()?.to_string_lossy().to_lowercase();
    if let Some(idxs) = by_name.get(&name_l) {
        for &i in idxs {
            if let Some(d) = entries.get(i) {
                if d.ultimate_format.is_some() {
                    return Some(d);
                }
            }
        }
    }

    let sp = src_path.to_string_lossy().replace('\\', "/").to_lowercase();
    if let Some(d) = default_icon.as_ref() {
        if sp.contains("/icon/") || sp.ends_with("_00.bntx") || sp.ends_with("_00_big.bntx") {
            return Some(d);
        }
    }
    None
}

fn already_converted(meta: &bntx::BntxMeta, donor: &BntxIndexEntry) -> bool {
    let Some(dfmt) = donor.ultimate_format.as_deref() else {
        return false;
    };
    let src_ult = ultimate_format(meta.format_type, meta.format_var);
    let Some(src_ult) = src_ult else {
        return false;
    };
    if (meta.width, meta.height) != (donor.width, donor.height) {
        return false;
    }
    if src_ult != dfmt {
        return false;
    }
    let src_no_mip = meta.mip_count <= 1;
    if src_no_mip != donor.no_mipmaps {
        return false;
    }
    if meta.data_length != donor.data_length {
        return false;
    }
    true
}

fn convert_one(
    src_bntx: &Path,
    dst_bntx: &Path,
    donor: &BntxIndexEntry,
    ultimate: &Path,
    allow_resize: bool,
    _progress: &ProgressSink,
) -> anyhow::Result<bool> {
    let Some(fmt) = donor.ultimate_format.as_deref() else {
        return Ok(false);
    };
    let donor_path = PathBuf::from(&donor.file_path);
    if !donor_path.is_file() {
        return Ok(false);
    }

    let tmp_base = dst_bntx.parent().unwrap_or(Path::new(".")).join("_tmp");
    fs::create_dir_all(&tmp_base)?;
    let td = tempfile::Builder::new()
        .prefix("svza_tex_")
        .tempdir_in(&tmp_base)?;
    let decoded_bmp = td.path().join("decoded.bmp");
    let resized_bmp = td.path().join("resized.bmp");
    let encoded_bntx = td.path().join("encoded.bntx");

    run_ultimate(ultimate, &[src_bntx, &decoded_bmp], None)?;
    let (sw, sh, rgba) = bmp::read_bmp_rgba(&decoded_bmp)?;
    let (tw, th) = (donor.width, donor.height);
    let (bmp_in, rgba2) = if (sw, sh) != (tw, th) {
        if !allow_resize {
            return Ok(false);
        }
        let rgba2 = bmp::resize_rgba_bilinear(sw, sh, &rgba, tw, th);
        bmp::write_bmp_rgba(&resized_bmp, tw, th, &rgba2)?;
        (resized_bmp.as_path(), rgba2)
    } else {
        (decoded_bmp.as_path(), rgba)
    };
    let _ = rgba2;

    let args = vec![bmp_in, encoded_bntx.as_path()];
    let mut extra = vec!["--format".to_string(), fmt.to_string()];
    if donor.no_mipmaps {
        extra.push("--no-mipmaps".to_string());
    }
    run_ultimate(ultimate, &args, Some(&extra))?;

    let (enc_data, _enc_off, enc_len) = extract_tex_data(&encoded_bntx)?;
    let donor_bytes = fs::read(&donor_path)?;
    let d_off = donor.base_offset;
    let d_len = donor.data_length;
    if d_off < 0 || d_len <= 0 {
        return Ok(false);
    }
    let d_off = d_off as usize;
    let d_len = d_len as usize;
    if d_off + d_len > donor_bytes.len() {
        return Ok(false);
    }
    if enc_len != d_len {
        return Ok(false);
    }

    let mut out = donor_bytes;
    out[d_off..d_off + d_len].copy_from_slice(&enc_data);
    atomic_write(dst_bntx, &out)?;
    Ok(true)
}

fn run_ultimate(ultimate: &Path, args: &[&Path], extra: Option<&[String]>) -> anyhow::Result<()> {
    let mut cmd = Command::new(ultimate);
    for a in args {
        cmd.arg(a);
    }
    if let Some(extra) = extra {
        for e in extra {
            cmd.arg(e);
        }
    }
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!(
            "ultimate_tex_cli failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

fn atomic_write(dst: &Path, data: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = PathBuf::from(format!("{}{}", dst.to_string_lossy(), ".tmp"));
    fs::write(&tmp, data)?;
    let _ = fs::remove_file(dst);
    fs::rename(&tmp, dst)?;
    Ok(())
}
