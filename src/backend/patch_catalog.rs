use crate::{
    fb::trpmcatalog::{self, AnimationInfo, CatalogEntryFull, LocatorInfo, SpeciesKey},
    progress::ProgressSink,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct PatchMon {
    pub key: SpeciesKey,
    pub pm: String,
    pub pm_variant: String,
}

pub fn patch_za_catalog(
    za_dump: &Path,
    out_root: &Path,
    mons: &[PatchMon],
    progress: &ProgressSink,
) -> anyhow::Result<PathBuf> {
    progress.phase_start("Patch ZA catalog");
    let in_path = za_dump
        .join("ik_pokemon")
        .join("catalog")
        .join("catalog")
        .join("poke_resource_table.trpmcatalog");
    if !in_path.is_file() {
        anyhow::bail!("ZA catalog not found at expected path: {in_path:?}");
    }

    let mut doc = trpmcatalog::read_doc(fs::read(&in_path)?)?;
    let mut index = HashMap::<SpeciesKey, usize>::new();
    for (i, e) in doc.entries.iter().enumerate() {
        index.insert(e.key, i);
    }

    let mut changed = 0usize;
    for m in mons {
        let base = format!("{}/{}", m.pm, m.pm_variant);
        let entry = CatalogEntryFull {
            key: m.key,
            model_path: format!("{base}/{}.trmdl", m.pm_variant),
            material_table_path: format!("{base}/{}.trmmt", m.pm_variant),
            config_path: format!("{base}/{}.trpokecfg", m.pm_variant),
            animations: vec![AnimationInfo {
                form_number: m.key.form as i16,
                path: format!("{base}/{}.tracn", m.pm_variant),
            }],
            locators: vec![
                LocatorInfo {
                    form_number: m.key.form as i16,
                    loc_index: 0,
                    loc_path: format!("{base}/{}_00000.trskl", m.pm_variant),
                },
                LocatorInfo {
                    form_number: m.key.form as i16,
                    loc_index: 1,
                    loc_path: format!("{base}/{}_20000.trskl", m.pm_variant),
                },
            ],
            icon_path: format!("{base}/{}_00.bntx", m.pm_variant),
            unk_id: 0,
            defence_path: format!("{base}/{}_defence.hkx", m.pm_variant),
        };

        if let Some(i) = index.get(&m.key).copied() {
            doc.entries[i] = entry;
        } else {
            doc.entries.push(entry);
        }
        changed += 1;
    }

    let out_path = out_root
        .join("ik_pokemon")
        .join("catalog")
        .join("catalog")
        .join("poke_resource_table.trpmcatalog");
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if out_path.is_file() {
        let bak = out_path.with_extension("trpmcatalog.pre_patch.bak");
        if !bak.exists() {
            fs::copy(&out_path, bak)?;
        }
    }

    let bin = trpmcatalog::write_doc(&doc)?;
    fs::write(&out_path, bin)?;

    progress.info(format!("[catalog] patched entries: {changed}"));
    progress.phase_end("Patch ZA catalog");
    Ok(out_path)
}
