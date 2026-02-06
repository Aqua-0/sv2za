use crate::{
    fb::{tracn, tralk},
    progress::ProgressSink,
};
use std::{fs, path::Path};

pub fn sv_style_disable_tralk(
    pm_variant_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    let pm = pm_variant_dir
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("unexpected pm dir: {pm_variant_dir:?}"))?
        .to_string_lossy()
        .to_string();

    let tracn = pm_variant_dir.join(format!("{pm}_base.tracn"));
    if tracn.is_file() {
        let mut b = fs::read(&tracn)?;
        let changed = tracn::strip_tralk_filenames_in_place(&mut b)?;
        if changed > 0 {
            fs::write(&tracn, b)?;
            progress.info(format!("[lookat] stripped .tralk refs: {pm} ({changed})"));
        }
    }

    let tralk_path = pm_variant_dir.join(format!("{pm}_base.tralk"));
    if tralk_path.is_file() {
        let bak = tralk_path.with_extension("tralk.sv.bak");
        if !bak.exists() {
            fs::copy(&tralk_path, &bak)?;
        }
        fs::remove_file(&tralk_path)?;
        progress.info(format!("[lookat] removed SV tralk: {pm}"));
    }

    Ok(())
}

pub fn za_patch_no_head_lookat(
    pm_variant_dir: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<()> {
    let pm = pm_variant_dir
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("unexpected pm dir: {pm_variant_dir:?}"))?
        .to_string_lossy()
        .to_string();
    let tralk_path = pm_variant_dir.join(format!("{pm}_base.tralk"));
    if !tralk_path.is_file() {
        progress.warn(format!("[lookat] missing ZA base.tralk to patch: {pm}"));
        return Ok(());
    }

    let bak = tralk_path.with_extension("tralk.pre_nohead.bak");
    if !bak.exists() {
        fs::copy(&tralk_path, &bak)?;
    }

    let mut b = fs::read(&tralk_path)?;
    let changed = tralk::patch_no_head_joint_rotation_in_place(&mut b)?;
    fs::write(&tralk_path, b)?;
    if changed == 0 {
        progress.warn(format!(
            "[lookat] did not find head JointRotation group: {pm}"
        ));
    } else {
        progress.info(format!("[lookat] patched no-head-look-at: {pm}"));
    }
    Ok(())
}
