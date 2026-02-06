use crate::{
    backend::{anim_sync, ensure, lookat, za_base},
    config::AppConfig,
    progress::ProgressSink,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub fn copy_pm_variants(
    poke_root: &Path,
    za_dump: &Path,
    out_root: &Path,
    cfg: &AppConfig,
    pm_variants: &[(String, String)],
    donor_by_target_pm_variant: Option<&std::collections::HashMap<String, String>>,
    progress: &ProgressSink,
) -> anyhow::Result<Vec<anim_sync::AnimSyncStats>> {
    progress.phase_start("Copy pm packages");

    let mut stats = Vec::new();
    let total = pm_variants.len().max(1) as u64;
    let mut done = 0u64;
    for (pm, pm_variant) in pm_variants {
        done += 1;
        progress.progress(done, total);

        let src = poke_root.join("data").join(pm).join(pm_variant);
        let dst = out_root
            .join("ik_pokemon")
            .join("data")
            .join(pm)
            .join(pm_variant);

        if !src.is_dir() {
            progress.warn(format!("missing src pm dir: {:?}", src));
            continue;
        }

        ensure_dir(&dst)?;
        copy_tree_missing_only(&src, &dst)?;

        if let Some(map) = donor_by_target_pm_variant {
            if let Some(donor_variant) = map.get(pm_variant) {
                za_base::overlay_from_donor(za_dump, donor_variant, &dst, progress)?;
            } else if cfg.use_za_base_config {
                za_base::overlay_from_donor(
                    za_dump,
                    &cfg.za_base_donor_pm_variant,
                    &dst,
                    progress,
                )?;
            }
            if cfg.no_head_look_at {
                lookat::za_patch_no_head_lookat(&dst, progress)?;
            }
        } else {
            if cfg.use_za_base_config {
                za_base::overlay_from_donor(
                    za_dump,
                    &cfg.za_base_donor_pm_variant,
                    &dst,
                    progress,
                )?;
                if cfg.no_head_look_at {
                    lookat::za_patch_no_head_lookat(&dst, progress)?;
                }
            } else {
                lookat::sv_style_disable_tralk(&dst, progress)?;
            }
        }

        let anim = anim_sync::sync_tracr_resources_from_sv(&dst, &src, progress)?;
        stats.push(anim);

        ensure_icons(&dst, pm_variant, progress)?;
        mirror_sv_motion_files_to_za_names(&dst, pm_variant)?;

        ensure::ensure_defence_hkx(za_dump, &cfg.za_base_donor_pm_variant, &dst, progress)?;
    }

    progress.phase_end("Copy pm packages");
    Ok(stats)
}

fn ensure_dir(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

fn copy_tree_missing_only(src: &Path, dst: &Path) -> anyhow::Result<()> {
    for entry in WalkDir::new(src).follow_links(false) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src)?;
        let out = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if out.exists() {
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &out)?;
    }
    Ok(())
}

fn ensure_icons(
    dst_pm_variant_dir: &Path,
    pm_variant: &str,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    let icon_dir = dst_pm_variant_dir.join("icon");
    let root_icon = dst_pm_variant_dir.join(format!("{pm_variant}_00.bntx"));
    let in_dir_icon = icon_dir.join(format!("{pm_variant}_00.bntx"));

    if !root_icon.exists() && in_dir_icon.exists() {
        fs::copy(&in_dir_icon, &root_icon)?;
    }

    let root_big = dst_pm_variant_dir.join(format!("{pm_variant}_00_big.bntx"));
    let in_dir_big = icon_dir.join(format!("{pm_variant}_00_big.bntx"));
    if !root_big.exists() && in_dir_big.exists() {
        fs::copy(&in_dir_big, &root_big)?;
    }

    let donor = pick_icon_donor(dst_pm_variant_dir, pm_variant)?;
    let Some(donor) = donor else {
        progress.warn(format!(
            "no icon donor found for {pm_variant} under {:?}",
            dst_pm_variant_dir
        ));
        return Ok(());
    };

    fs::create_dir_all(&icon_dir)?;
    for v in ["0", "1"] {
        let n = format!("{pm_variant}_00_{v}.bntx");
        let dst1 = icon_dir.join(&n);
        if !dst1.exists() {
            fs::copy(&donor, &dst1)?;
        }
        let dst2 = dst_pm_variant_dir.join(&n);
        if !dst2.exists() {
            fs::copy(&donor, &dst2)?;
        }
    }

    Ok(())
}

fn pick_icon_donor(dst_pm_variant_dir: &Path, pm_variant: &str) -> anyhow::Result<Option<PathBuf>> {
    let icon_dir = dst_pm_variant_dir.join("icon");
    let candidates = [
        dst_pm_variant_dir.join(format!("{pm_variant}_00.bntx")),
        icon_dir.join(format!("{pm_variant}_00.bntx")),
        dst_pm_variant_dir.join(format!("{pm_variant}_00_0.bntx")),
        icon_dir.join(format!("{pm_variant}_00_0.bntx")),
        dst_pm_variant_dir.join(format!("{pm_variant}_00_1.bntx")),
        icon_dir.join(format!("{pm_variant}_00_1.bntx")),
    ];
    for c in candidates {
        if c.is_file() {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

fn mirror_sv_motion_files_to_za_names(
    dst_pm_variant_dir: &Path,
    pm_variant: &str,
) -> anyhow::Result<()> {
    for ext in ["tranm", "tracm", "traef"] {
        let Ok(rd) = fs::read_dir(dst_pm_variant_dir) else {
            continue;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else {
                continue;
            };
            if !ft.is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let pat = format!("{pm_variant}_2");
            if !name.starts_with(&pat) || !name.ends_with(&format!(".{ext}")) {
                continue;
            }
            // pmXXXX_00_00_20030_foo.ext
            let parts = name.splitn(5, '_').collect::<Vec<_>>();
            if parts.len() < 5 {
                continue;
            }
            let Ok(motion_id) = parts[3].parse::<i32>() else {
                continue;
            };
            if !(20000..=29999).contains(&motion_id) {
                continue;
            }
            let za0 = motion_id - 20000;
            let za1 = 10000 + za0;
            let prefix = format!("{}_{}_{}", parts[0], parts[1], parts[2]);
            let suffix = parts[4];

            let dst0 = dst_pm_variant_dir.join(format!("{prefix}_{za0:05}_{suffix}"));
            let dst1 = dst_pm_variant_dir.join(format!("{prefix}_{za1:05}_{suffix}"));
            if !dst0.exists() {
                fs::copy(e.path(), dst0)?;
            }
            if !dst1.exists() {
                fs::copy(e.path(), dst1)?;
            }
        }
    }
    Ok(())
}
