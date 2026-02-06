use crate::fb::raw::FbBuf;
use crate::progress::ProgressSink;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct AnimSyncStats {
    pub pm_variant: String,
    pub had_tracr: bool,
    pub tracks: usize,
    pub refs: usize,
    pub filled: usize,
    pub missing_src: usize,
    pub missing_after: usize,
    pub error: String,
}

pub fn sync_tracr_resources_from_sv(
    target_pm_dir: &Path,
    sv_pm_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<AnimSyncStats> {
    let pm_variant = target_pm_dir
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("unexpected pm dir: {target_pm_dir:?}"))?
        .to_string_lossy()
        .to_string();

    let mut stats = AnimSyncStats {
        pm_variant: pm_variant.clone(),
        had_tracr: false,
        tracks: 0,
        refs: 0,
        filled: 0,
        missing_src: 0,
        missing_after: 0,
        error: String::new(),
    };

    let tracr_path = target_pm_dir.join(format!("{pm_variant}_base.tracr"));
    if !tracr_path.is_file() {
        return Ok(stats);
    }
    let b = fs::read(&tracr_path)?;
    let fb = FbBuf::new(b);
    stats.had_tracr = true;

    let root = match fb.root_table_pos() {
        Ok(x) => x,
        Err(e) => {
            stats.error = format!("tracr parse: {e}");
            return Ok(stats);
        }
    };
    let root_vt = fb.vtable_pos(root)?;
    let Some(track_list_pos) = fb.table_field_table_pos(root, root_vt, 0)? else {
        return Ok(stats);
    };
    let tl_vt = fb.vtable_pos(track_list_pos)?;
    let tracks = fb
        .table_field_vec_of_tables(track_list_pos, tl_vt, 0)?
        .unwrap_or_default();
    stats.tracks = tracks.len();

    for tpos in &tracks {
        let tvt = fb.vtable_pos(*tpos)?;
        let track_name = fb.table_field_string(*tpos, tvt, 0)?.unwrap_or_default();
        let (za_id, suffix) = parse_track_name(&track_name);

        let Some(tr_res_pos) = fb.table_field_table_pos(*tpos, tvt, 3)? else {
            continue;
        };
        let rvt = fb.vtable_pos(tr_res_pos)?;

        for (slot, ext) in [(0usize, "tranm"), (1, "tracm"), (2, "traef")] {
            let Some(res_pos) = fb.table_field_table_pos(tr_res_pos, rvt, slot)? else {
                continue;
            };
            let res_vt = fb.vtable_pos(res_pos)?;
            let Some(filename) = fb.table_field_string(res_pos, res_vt, 0)? else {
                continue;
            };
            if !filename.ends_with(ext) {
                continue;
            }
            stats.refs += 1;
            let dst = target_pm_dir.join(&filename);
            if dst.is_file() {
                continue;
            }

            let mut src = sv_pm_dir.join(&filename);
            if !src.is_file() {
                if let Some(za_id) = za_id {
                    if let Some(sv_id) = src_id_from_za_id(za_id) {
                        let cand = format!("{pm_variant}_{sv_id:05}_{suffix}.{ext}");
                        let p = sv_pm_dir.join(&cand);
                        if p.is_file() {
                            src = p;
                        } else if let Some(fb) = pick_fallback(sv_pm_dir, pm_variant.as_str(), ext)
                        {
                            src = fb;
                        }
                    } else if let Some(fb) = pick_fallback(sv_pm_dir, pm_variant.as_str(), ext) {
                        src = fb;
                    }
                } else if let Some(fb) = pick_fallback(sv_pm_dir, pm_variant.as_str(), ext) {
                    src = fb;
                }
            }

            if src.is_file() {
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::copy(src, &dst)?;
                stats.filled += 1;
            } else {
                stats.missing_src += 1;
            }
        }
    }

    // audit missing after
    for tpos in &tracks {
        let tvt = fb.vtable_pos(*tpos)?;
        let Some(tr_res_pos) = fb.table_field_table_pos(*tpos, tvt, 3)? else {
            continue;
        };
        let rvt = fb.vtable_pos(tr_res_pos)?;
        for (slot, ext) in [(0usize, "tranm"), (1, "tracm"), (2, "traef")] {
            let Some(res_pos) = fb.table_field_table_pos(tr_res_pos, rvt, slot)? else {
                continue;
            };
            let res_vt = fb.vtable_pos(res_pos)?;
            let Some(filename) = fb.table_field_string(res_pos, res_vt, 0)? else {
                continue;
            };
            if !filename.ends_with(ext) {
                continue;
            }
            if !target_pm_dir.join(&filename).is_file() {
                stats.missing_after += 1;
            }
        }
    }

    if stats.filled > 0 || stats.missing_src > 0 || stats.missing_after > 0 {
        progress.info(format!(
            "[anim] {pm_variant}: tracks={} refs={} filled={} missing_src={} missing_after={}",
            stats.tracks, stats.refs, stats.filled, stats.missing_src, stats.missing_after
        ));
    }

    Ok(stats)
}

fn parse_track_name(track_name: &str) -> (Option<i32>, String) {
    // expect "00000_suffix..."
    if track_name.len() < 7 {
        return (None, String::new());
    }
    let (id_s, rest) = track_name.split_at(5);
    if !id_s.chars().all(|c| c.is_ascii_digit()) {
        return (None, String::new());
    }
    let rest = rest.strip_prefix('_').unwrap_or(rest);
    let Ok(id) = id_s.parse::<i32>() else {
        return (None, String::new());
    };
    (Some(id), rest.to_string())
}

fn src_id_from_za_id(za_id: i32) -> Option<i32> {
    if (0..=9999).contains(&za_id) {
        return Some(20000 + za_id);
    }
    if (10000..=19999).contains(&za_id) {
        return Some(20000 + (za_id - 10000));
    }
    if (20000..=29999).contains(&za_id) {
        return Some(za_id);
    }
    None
}

fn pick_fallback(sv_pm_dir: &Path, pm_variant: &str, ext: &str) -> Option<std::path::PathBuf> {
    let candidates = [
        format!("{pm_variant}_20000_defaultwait01_loop.{ext}"),
        format!("{pm_variant}_20010_defaultidle01.{ext}"),
        format!("{pm_variant}_20001_battlewait01_loop.{ext}"),
        format!("{pm_variant}_00000_defaultwait01_loop.{ext}"),
        format!("{pm_variant}_00010_defaultidle01.{ext}"),
        format!("{pm_variant}_00001_battlewait01_loop.{ext}"),
    ];
    for c in candidates {
        let p = sv_pm_dir.join(&c);
        if p.is_file() {
            return Some(p);
        }
    }
    let pat2 = format!("{pm_variant}_2????_*.{ext}");
    for e in glob_in_dir(sv_pm_dir, &pat2) {
        return Some(e);
    }
    let pat_any = format!("{pm_variant}_?????_*.{ext}");
    for e in glob_in_dir(sv_pm_dir, &pat_any) {
        return Some(e);
    }
    None
}

fn glob_in_dir(dir: &Path, pat: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if glob_match(pat, &name) {
            out.push(e.path());
        }
    }
    out.sort();
    out
}

fn glob_match(pat: &str, name: &str) -> bool {
    // tiny matcher: supports '?' and '*' only
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut match_i) = (None::<usize>, 0usize);
    let p = pat.as_bytes();
    let n = name.as_bytes();
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            match_i = ni;
            pi += 1;
        } else if let Some(si) = star {
            pi = si + 1;
            match_i += 1;
            ni = match_i;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}
