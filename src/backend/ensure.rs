use crate::progress::ProgressSink;
use std::{fs, path::Path};
use walkdir::WalkDir;

pub fn ensure_defence_hkx(
    za_dump: &Path,
    donor_pm_variant: &str,
    target_pm_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    let pm_variant = target_pm_dir
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("unexpected pm dir: {target_pm_dir:?}"))?
        .to_string_lossy()
        .to_string();
    let dst = target_pm_dir.join(format!("{pm_variant}_defence.hkx"));
    if dst.is_file() {
        return Ok(());
    }

    let donor_pm = donor_pm_variant
        .split_once('_')
        .map(|(a, _)| a)
        .unwrap_or(donor_pm_variant);

    let direct = za_dump
        .join("ik_pokemon")
        .join("data")
        .join(donor_pm)
        .join(donor_pm_variant)
        .join(format!("{donor_pm_variant}_defence.hkx"));

    let src = if direct.is_file() {
        direct
    } else {
        let root = za_dump.join("ik_pokemon").join("data");
        let want = format!("{donor_pm_variant}_defence.hkx");
        let mut found = None;
        for e in WalkDir::new(&root).follow_links(false) {
            let e = e?;
            if !e.file_type().is_file() {
                continue;
            }
            if e.file_name().to_string_lossy() == want {
                found = Some(e.path().to_path_buf());
                break;
            }
        }
        if let Some(p) = found {
            p
        } else {
            progress.warn(format!(
                "[hkx] donor defence hkx not found for {donor_pm_variant}"
            ));
            return Ok(());
        }
    };

    fs::copy(src, &dst)?;
    progress.info(format!("[hkx] copied defence hkx: {pm_variant}"));
    Ok(())
}
