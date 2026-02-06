use crate::progress::ProgressSink;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn overlay_from_donor(
    za_dump: &Path,
    donor_pm_variant: &str,
    out_pm_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    let target_pm_variant = out_pm_dir
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("unexpected pm dir: {out_pm_dir:?}"))?
        .to_string_lossy()
        .to_string();
    let donor_pm = donor_pm_variant
        .split_once('_')
        .map(|(a, _)| a)
        .unwrap_or(donor_pm_variant);

    let donor_dir = za_dump
        .join("ik_pokemon")
        .join("data")
        .join(donor_pm)
        .join(donor_pm_variant);
    if !donor_dir.is_dir() {
        anyhow::bail!("ZA base-config donor folder missing: {donor_dir:?}");
    }

    let donor_b = donor_pm_variant.as_bytes();
    let target_b = target_pm_variant.as_bytes();
    if donor_b.len() != target_b.len() {
        anyhow::bail!(
            "donor/target pm_variant length mismatch: {donor_pm_variant} vs {target_pm_variant}"
        );
    }

    let mut copied = Vec::<PathBuf>::new();

    let Ok(rd) = fs::read_dir(&donor_dir) else {
        return Ok(());
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let mut dst: Option<PathBuf> = None;
        if name == format!("{donor_pm_variant}.tracn") {
            dst = Some(out_pm_dir.join(format!("{target_pm_variant}.tracn")));
        } else if name.starts_with(&format!("{donor_pm_variant}_base.")) {
            let tail = &name[donor_pm_variant.len()..];
            dst = Some(out_pm_dir.join(format!("{target_pm_variant}{tail}")));
        } else if name.starts_with(&format!("{donor_pm_variant}_")) && name.ends_with(".trcrv") {
            let tail = &name[donor_pm_variant.len()..];
            dst = Some(out_pm_dir.join(format!("{target_pm_variant}{tail}")));
        }

        let Some(dst) = dst else { continue };
        copy_overwrite_backup(e.path(), &dst, ".pre_za_base.bak")?;
        copied.push(dst);
    }

    for extra in [
        format!("{donor_pm_variant}_base_motion_detector.trmdd"),
        format!("{donor_pm_variant}_defence.hkx"),
        format!("{donor_pm_variant}_oybn.trpokecfg"),
    ] {
        let src = donor_dir.join(&extra);
        if !src.is_file() {
            continue;
        }
        let tail = &extra[donor_pm_variant.len()..];
        let dst = out_pm_dir.join(format!("{target_pm_variant}{tail}"));
        copy_overwrite_backup(src, &dst, ".pre_za_base.bak")?;
    }

    let donor_loc = donor_dir.join("locators");
    if donor_loc.is_dir() {
        for extra in [
            format!("{donor_pm_variant}_00000_eff.trskl"),
            format!("{donor_pm_variant}_10000_eff.trskl"),
        ] {
            let src = donor_loc.join(&extra);
            if !src.is_file() {
                continue;
            }
            let tail = &extra[donor_pm_variant.len()..];
            let dst = out_pm_dir
                .join("locators")
                .join(format!("{target_pm_variant}{tail}"));
            copy_overwrite_backup(src, &dst, ".pre_za_base.bak")?;
        }
    }

    // Retarget embedded names (fixed-width)
    for p in copied {
        let Ok(b) = fs::read(&p) else {
            continue;
        };
        if !b.windows(donor_b.len()).any(|w| w == donor_b) {
            continue;
        }
        let replaced = replace_all_bytes(&b, donor_b, target_b);
        let _ = fs::write(&p, replaced);
    }

    progress.info(format!(
        "za base overlay: donor={} -> {}",
        donor_pm_variant, target_pm_variant
    ));
    Ok(())
}

fn copy_overwrite_backup(src: PathBuf, dst: &Path, bak_suffix: &str) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if dst.exists() {
        let bak = PathBuf::from(format!("{}{}", dst.to_string_lossy(), bak_suffix));
        if !bak.exists() {
            fs::copy(dst, bak)?;
        }
    }
    fs::copy(src, dst)?;
    Ok(())
}

fn replace_all_bytes(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    if from.is_empty() || from.len() != to.len() {
        return hay.to_vec();
    }
    let mut out = hay.to_vec();
    let mut i = 0usize;
    while i + from.len() <= out.len() {
        if &out[i..i + from.len()] == from {
            out[i..i + to.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
    out
}
