use crate::template::{preferred_template_dirs, DonorTemplate, Key, TemplateStore};
use crate::{
    cancel::CancelToken,
    config::AppConfig,
    paths::{canonicalish, detect_sv_layout, find_under},
    progress::ProgressSink,
};
use serde_json;
use std::collections::{BTreeMap, HashMap, HashSet as StdHashSet};

mod anim_sync;
mod catalog;
mod copy_pm;
mod ensure;
mod flatc;
mod lookat;
pub mod names;
mod param_arrays;
mod patch_catalog;
mod personal;
mod textures;
mod za_base;

pub fn run(cfg: &AppConfig, progress: ProgressSink, cancel: CancelToken) -> anyhow::Result<()> {
    progress.phase_start("Validate paths");

    let sv_root = cfg
        .sv_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("SV root not set"))?
        .clone();
    let za_dump = cfg
        .za_dump
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("ZA dump not set"))?
        .clone();
    let out_root = cfg
        .out_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Output root not set"))?
        .clone();

    let sv_root = canonicalish(&sv_root);
    let za_dump = canonicalish(&za_dump);
    let out_root = canonicalish(&out_root);

    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    let mut done = 0u64;
    let total = 6u64;
    let mut bump = |progress: &ProgressSink| {
        done += 1;
        progress.progress(done, total);
    };

    if !sv_root.is_dir() {
        progress.error(format!("SV root is not a directory: {sv_root:?}"));
        anyhow::bail!("SV root is not a directory: {sv_root:?}");
    }
    bump(&progress);

    if !za_dump.is_dir() {
        progress.error(format!("ZA dump is not a directory: {za_dump:?}"));
        anyhow::bail!("ZA dump is not a directory: {za_dump:?}");
    }
    bump(&progress);

    if !out_root.exists() {
        progress.info(format!("output folder does not exist yet: {out_root:?}"));
    }
    bump(&progress);

    let Some((layout, poke_root)) = detect_sv_layout(&sv_root) else {
        progress.error(format!(
            "SV root must contain either 'pokemon/' or 'ik_pokemon/': {sv_root:?}"
        ));
        anyhow::bail!("SV root must contain either 'pokemon/' or 'ik_pokemon/': {sv_root:?}");
    };

    progress.info(format!("SV layout: {:?} ({:?})", layout, poke_root));
    progress.info(format!("ZA dump: {:?}", za_dump));
    progress.info(format!("Output: {:?}", out_root));
    bump(&progress);

    if cfg.texture_convert {
        let ultimate = cfg.ultimate_tex_cli.as_ref().ok_or_else(|| {
            anyhow::anyhow!("texture_convert enabled but ultimate_tex_cli not set")
        })?;
        progress.info(format!("ultimate_tex_cli: {:?}", canonicalish(ultimate)));
    }
    bump(&progress);

    let za_catalog_rel = "ik_pokemon/catalog/catalog/poke_resource_table.trpmcatalog";
    match find_under(&za_dump, za_catalog_rel, "poke_resource_table.trpmcatalog") {
        Ok(p) => progress.info(format!("ZA catalog: {p:?}")),
        Err(e) => progress.warn(format!("ZA catalog not found yet ({za_catalog_rel}): {e}")),
    }

    let sv_catalog_rel = "catalog/catalog/poke_resource_table.trpmcatalog";
    match find_under(
        &poke_root,
        sv_catalog_rel,
        "poke_resource_table.trpmcatalog",
    ) {
        Ok(p) => progress.info(format!("SV catalog: {p:?}")),
        Err(e) => progress.warn(format!("SV catalog not found yet ({sv_catalog_rel}): {e}")),
    }
    bump(&progress);

    progress.phase_end("Validate paths");

    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    let (selection, donor_by_target_pm_variant, donor_by_species): (
        catalog::CatalogSelection,
        Option<HashMap<String, String>>,
        Option<BTreeMap<u16, u16>>,
    ) = if cfg.legacy_mode {
        let selection = catalog::select_missing_in_za(
            &poke_root,
            &za_dump,
            cfg.skip_pokemon_already_in_za,
            &progress,
        )?;
        (selection, None, None)
    } else {
        let tpl = load_autosave_template(cfg).unwrap_or_default();

        let keys: StdHashSet<_> = tpl
            .selected_targets
            .iter()
            .copied()
            .map(|k| crate::fb::trpmcatalog::SpeciesKey::from(k))
            .collect();

        let selection = if keys.is_empty() {
            catalog::select_missing_in_za(
                &poke_root,
                &za_dump,
                cfg.skip_pokemon_already_in_za,
                &progress,
            )?
        } else {
            catalog::select_by_keys(
                &poke_root,
                &za_dump,
                &keys,
                tpl.include_targets_already_in_za,
                &progress,
            )?
        };

        let za_model_path_by_key = catalog::read_catalog_map(&selection.za_catalog)?;
        let donor_map = tpl.assignment_map();

        let default_donor = tpl
            .default_donor
            .map(crate::fb::trpmcatalog::SpeciesKey::from);

        let mut donor_by_target_pm_variant = HashMap::<String, String>::new();
        let mut donor_by_species = BTreeMap::<u16, u16>::new();

        for e in &selection.entries {
            let tkey = Key::from(e.key);
            let donor_key = donor_map
                .get(&tkey)
                .copied()
                .or(default_donor.map(Key::from));
            let Some(donor_key) = donor_key else {
                continue;
            };
            let donor_species = donor_key.species;
            donor_by_species.insert(e.key.species, donor_species);

            let dkey = crate::fb::trpmcatalog::SpeciesKey::from(donor_key);
            let Some(model_path) = za_model_path_by_key.get(&dkey) else {
                continue;
            };
            let Some((_, donor_pm_variant)) = parse_pm_variant(model_path) else {
                continue;
            };
            donor_by_target_pm_variant.insert(e.pm_variant.clone(), donor_pm_variant);
        }

        let donor_by_target_pm_variant =
            (!donor_by_target_pm_variant.is_empty()).then_some(donor_by_target_pm_variant);
        let donor_by_species = (!donor_by_species.is_empty()).then_some(donor_by_species);

        (selection, donor_by_target_pm_variant, donor_by_species)
    };
    progress.info(format!(
        "catalogs: sv={:?} za={:?}",
        selection.sv_catalog, selection.za_catalog
    ));
    progress.info(format!(
        "selection: {} species entries, {} pm_variants",
        selection.entries.len(),
        selection.unique_pm_variants.len()
    ));
    if let Some(e) = selection.entries.get(0) {
        progress.info(format!(
            "example: species={} form={} gender={} -> {}/{} ({})",
            e.key.species, e.key.form, e.key.gender, e.pm, e.pm_variant, e.model_path
        ));
    }
    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    let anim_stats = copy_pm::copy_pm_variants(
        &poke_root,
        &za_dump,
        &out_root,
        cfg,
        &selection.unique_pm_variants,
        donor_by_target_pm_variant.as_ref(),
        &progress,
    )?;

    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    if cfg.generate_reports {
        // report
        {
            use std::fs;
            let report_dir = out_root.join("_report");
            let _ = fs::create_dir_all(&report_dir);
            let path = report_dir.join("anim_sync.json");
            if let Ok(text) = serde_json::to_string_pretty(&anim_stats) {
                let _ = fs::write(&path, text + "\n");
                progress.info(format!("[report] wrote {:?}", path));
            }
        }
    } else {
        progress.info("[report] disabled; skipping anim_sync.json");
    }

    let mons = selection
        .entries
        .iter()
        .map(|e| patch_catalog::PatchMon {
            key: e.key,
            pm: e.pm.clone(),
            pm_variant: e.pm_variant.clone(),
        })
        .collect::<Vec<_>>();
    let _out_catalog = patch_catalog::patch_za_catalog(&za_dump, &out_root, &mons, &progress)?;

    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    let mut new_species = std::collections::HashSet::<u16>::new();
    let mut enable_keys = std::collections::HashSet::<(u16, u16)>::new();
    let mut converted = Vec::new();
    for e in &selection.entries {
        new_species.insert(e.key.species);
        enable_keys.insert((e.key.species, e.key.form));
        converted.push(names::ConvertedMon {
            species: e.key.species,
            form: e.key.form,
            gender: e.key.gender,
            name: String::new(),
            pm: e.pm.clone(),
            pm_variant: e.pm_variant.clone(),
        });
    }

    if let Some(flatc_exe) = cfg.flatc.as_ref() {
        if let Some(map) = donor_by_species.as_ref() {
            param_arrays::patch_param_arrays_per_species(
                flatc_exe, &za_dump, &out_root, map, &progress,
            )?;
        } else {
            param_arrays::patch_param_arrays(
                flatc_exe,
                &za_dump,
                &out_root,
                cfg.donor_dev,
                &new_species,
                &progress,
            )?;
        }

        if let Some(pknx_dir) = cfg.pknx_personal_dir.as_ref() {
            personal::patch_personal_array_present(
                flatc_exe,
                &za_dump,
                &out_root,
                pknx_dir,
                &enable_keys,
                &progress,
            )?;
        } else {
            progress.warn("[personal] pkNX personal dir not set; skipping personal patch");
        }
    } else {
        progress.warn("[param/personal] flatc not set; skipping param + personal patch");
    }

    if cfg.generate_reports {
        let _names_report = names::write_converted_names_report(
            &za_dump,
            &out_root,
            &converted,
            &cfg.language,
            &progress,
        )?;
    } else {
        progress.info("[report] disabled; skipping converted names report");
    }

    if cancel.is_canceled() {
        progress.warn("canceled");
        return Ok(());
    }

    textures::convert_textures_if_enabled(cfg, &za_dump, &out_root, &progress)?;
    Ok(())
}

fn load_autosave_template(cfg: &AppConfig) -> anyhow::Result<DonorTemplate> {
    let _ = cfg;
    for dir in preferred_template_dirs() {
        let store = TemplateStore::new(dir);
        let path = store.autosave_path();
        if path.is_file() {
            return Ok(store.load_or_default(Some(&path)));
        }
    }
    Ok(DonorTemplate::default())
}

fn parse_pm_variant(model_path: &str) -> Option<(String, String)> {
    let mp = model_path.replace('\\', "/");
    let mut parts = mp.split('/').filter(|s| !s.is_empty());
    let pm = parts.next()?.to_string();
    let pm_variant = parts.next()?.to_string();
    Some((pm, pm_variant))
}
