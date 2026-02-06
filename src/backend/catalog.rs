use crate::{
    fb::trpmcatalog::{self, CatalogEntryLite, SpeciesKey},
    paths::find_under,
    progress::ProgressSink,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct SelectedMon {
    pub key: SpeciesKey,
    pub pm: String,
    pub pm_variant: String,
    pub model_path: String,
}

#[derive(Debug, Clone)]
pub struct CatalogSelection {
    pub sv_catalog: PathBuf,
    pub za_catalog: PathBuf,
    pub entries: Vec<SelectedMon>,
    pub unique_pm_variants: Vec<(String, String)>,
}

pub fn select_missing_in_za(
    poke_root: &Path,
    za_dump: &Path,
    skip_already_in_za: bool,
    progress: &ProgressSink,
) -> anyhow::Result<CatalogSelection> {
    progress.phase_start("Catalog & selection");

    let sv_catalog = find_under(
        poke_root,
        "catalog/catalog/poke_resource_table.trpmcatalog",
        "poke_resource_table.trpmcatalog",
    )?;
    let za_catalog = find_under(
        za_dump,
        "ik_pokemon/catalog/catalog/poke_resource_table.trpmcatalog",
        "poke_resource_table.trpmcatalog",
    )?;

    progress.info(format!("SV catalog: {:?}", sv_catalog));
    progress.info(format!("ZA catalog: {:?}", za_catalog));

    let sv_entries = read_catalog(&sv_catalog)?;
    let za_entries = read_catalog(&za_catalog)?;

    let existing_pm_variants = scan_existing_pm_variants(poke_root);
    if existing_pm_variants.is_empty() {
        progress.warn(format!(
            "no pm variants found under {:?}",
            poke_root.join("data")
        ));
    }

    let za_keys: HashSet<SpeciesKey> = za_entries.iter().map(|e| e.key).collect();
    let mut filtered = Vec::new();
    let mut missing_assets = Vec::new();

    if skip_already_in_za {
        progress.info("selection: skipping mons already present in ZA catalog");
    } else {
        progress.warn("selection: including mons already present in ZA catalog");
    }

    for e in sv_entries {
        let Some((pm, pm_variant)) = parse_pm_from_model_path(&e.model_path) else {
            continue;
        };
        if !existing_pm_variants.contains(&(pm.clone(), pm_variant.clone())) {
            missing_assets.push((pm, pm_variant));
            continue;
        }
        if skip_already_in_za && za_keys.contains(&e.key) {
            continue;
        }
        filtered.push(SelectedMon {
            key: e.key,
            pm,
            pm_variant,
            model_path: e.model_path,
        });
    }

    if !missing_assets.is_empty() {
        missing_assets.sort();
        missing_assets.dedup();
        let show = missing_assets.iter().take(20).cloned().collect::<Vec<_>>();
        progress.warn(format!(
            "SV catalog pm_variants missing assets: {} (first 20): {:?}",
            missing_assets.len(),
            show
        ));
    }

    let mut uniq = HashSet::<(String, String)>::new();
    for e in &filtered {
        uniq.insert((e.pm.clone(), e.pm_variant.clone()));
    }
    let mut unique_pm_variants = uniq.into_iter().collect::<Vec<_>>();
    unique_pm_variants.sort();

    progress.info(format!(
        "selected species entries: {} (unique pm_variants={})",
        filtered.len(),
        unique_pm_variants.len()
    ));
    progress.phase_end("Catalog & selection");

    Ok(CatalogSelection {
        sv_catalog,
        za_catalog,
        entries: filtered,
        unique_pm_variants,
    })
}

pub fn select_by_keys(
    poke_root: &Path,
    za_dump: &Path,
    keys: &HashSet<SpeciesKey>,
    include_already_in_za: bool,
    progress: &ProgressSink,
) -> anyhow::Result<CatalogSelection> {
    progress.phase_start("Catalog & selection");

    let sv_catalog = find_under(
        poke_root,
        "catalog/catalog/poke_resource_table.trpmcatalog",
        "poke_resource_table.trpmcatalog",
    )?;
    let za_catalog = find_under(
        za_dump,
        "ik_pokemon/catalog/catalog/poke_resource_table.trpmcatalog",
        "poke_resource_table.trpmcatalog",
    )?;

    let sv_entries = read_catalog(&sv_catalog)?;
    let za_entries = read_catalog(&za_catalog)?;
    let za_keys: HashSet<SpeciesKey> = za_entries.iter().map(|e| e.key).collect();

    let existing_pm_variants = scan_existing_pm_variants(poke_root);

    let mut filtered = Vec::new();
    for e in sv_entries {
        if !keys.contains(&e.key) {
            continue;
        }
        if !include_already_in_za && za_keys.contains(&e.key) {
            continue;
        }
        let Some((pm, pm_variant)) = parse_pm_from_model_path(&e.model_path) else {
            continue;
        };
        if !existing_pm_variants.contains(&(pm.clone(), pm_variant.clone())) {
            continue;
        }
        filtered.push(SelectedMon {
            key: e.key,
            pm,
            pm_variant,
            model_path: e.model_path,
        });
    }

    let mut uniq = HashSet::<(String, String)>::new();
    for e in &filtered {
        uniq.insert((e.pm.clone(), e.pm_variant.clone()));
    }
    let mut unique_pm_variants = uniq.into_iter().collect::<Vec<_>>();
    unique_pm_variants.sort();

    progress.info(format!(
        "selected keys: {} (unique pm_variants={})",
        filtered.len(),
        unique_pm_variants.len()
    ));
    progress.phase_end("Catalog & selection");

    Ok(CatalogSelection {
        sv_catalog,
        za_catalog,
        entries: filtered,
        unique_pm_variants,
    })
}

pub fn read_catalog_map(
    catalog_path: &Path,
) -> anyhow::Result<std::collections::HashMap<SpeciesKey, String>> {
    let entries = read_catalog(catalog_path)?;
    let mut out = std::collections::HashMap::new();
    for e in entries {
        out.insert(e.key, e.model_path);
    }
    Ok(out)
}

fn read_catalog(path: &Path) -> anyhow::Result<Vec<CatalogEntryLite>> {
    let b = fs::read(path)?;
    trpmcatalog::read_entries(b)
}

fn parse_pm_from_model_path(model_path: &str) -> Option<(String, String)> {
    let mp = model_path.replace('\\', "/");
    let mut parts = mp.split('/').filter(|s| !s.is_empty());
    let pm = parts.next()?.to_string();
    let pm_variant = parts.next()?.to_string();
    if !pm.starts_with("pm") || pm.len() != 6 {
        return None;
    }
    Some((pm, pm_variant))
}

fn scan_existing_pm_variants(poke_root: &Path) -> HashSet<(String, String)> {
    let data_dir = poke_root.join("data");
    let mut out = HashSet::new();
    let Ok(pm_dirs) = fs::read_dir(&data_dir) else {
        return out;
    };

    for pm_dir in pm_dirs.flatten() {
        let Ok(ft) = pm_dir.file_type() else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let pm = pm_dir.file_name().to_string_lossy().to_string();
        if !is_pm_dir(&pm) {
            continue;
        }
        let Ok(variants) = fs::read_dir(pm_dir.path()) else {
            continue;
        };
        for v in variants.flatten() {
            let Ok(vt) = v.file_type() else {
                continue;
            };
            if !vt.is_dir() {
                continue;
            }
            let name = v.file_name().to_string_lossy().to_string();
            if !name.starts_with(&(pm.clone() + "_")) {
                continue;
            }
            if name.split('_').count() < 3 {
                continue;
            }
            out.insert((pm.clone(), name));
        }
    }
    out
}

fn is_pm_dir(name: &str) -> bool {
    if name.len() != 6 {
        return false;
    }
    let b = name.as_bytes();
    if b[0] != b'p' || b[1] != b'm' {
        return false;
    }
    b[2..].iter().all(|c| c.is_ascii_digit())
}
