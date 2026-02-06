use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use clap::Parser;

#[derive(Debug, Parser)]
struct Args {
    /// One or more `poke_resource_table.trpmcatalog` paths
    #[arg(required = true)]
    catalogs: Vec<PathBuf>,

    /// Optional root to validate that referenced files exist
    /// If omitted, uses `<catalog>/../../data` (ik_pokemon/data) when possible
    #[arg(long)]
    data_root: Option<PathBuf>,

    /// Print per-entry details (can be noisy)
    #[arg(long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    for cat in &args.catalogs {
        let data_root = args.data_root.clone().or_else(|| default_data_root(cat));

        println!("== {}", cat.display());
        inspect_one(cat, data_root.as_deref(), args.verbose)?;
        println!();
    }

    if args.catalogs.len() == 2 {
        println!("== Diff");
        diff_two(&args.catalogs[0], &args.catalogs[1])?;
    }

    Ok(())
}

fn default_data_root(catalog: &Path) -> Option<PathBuf> {
    // .../ik_pokemon/catalog/catalog/poke_resource_table.trpmcatalog
    let p = catalog.parent()?.parent()?.parent()?;
    if p.file_name()?.to_string_lossy() != "ik_pokemon" {
        return None;
    }
    Some(p.join("data"))
}

fn inspect_one(catalog: &Path, data_root: Option<&Path>, verbose: bool) -> anyhow::Result<()> {
    let b = std::fs::read(catalog).with_context(|| format!("read {}", catalog.display()))?;
    let doc = svza::fb::trpmcatalog::read_doc(b).context("parse trpmcatalog")?;

    println!("version: {}", doc.version);
    println!("entries: {}", doc.entries.len());

    let mut dupes: BTreeMap<(u16, u16, u8), usize> = BTreeMap::new();
    let mut anim_lens: BTreeMap<usize, usize> = BTreeMap::new();
    let mut loc_lens: BTreeMap<usize, usize> = BTreeMap::new();
    let mut empty_fields = 0usize;
    let mut missing_files = 0usize;

    for e in &doc.entries {
        let k = (e.key.species, e.key.form, e.key.gender);
        *dupes.entry(k).or_insert(0) += 1;
        *anim_lens.entry(e.animations.len()).or_insert(0) += 1;
        *loc_lens.entry(e.locators.len()).or_insert(0) += 1;

        let fields: [(&str, &str); 5] = [
            ("model_path", e.model_path.as_str()),
            ("material_table_path", e.material_table_path.as_str()),
            ("config_path", e.config_path.as_str()),
            ("icon_path", e.icon_path.as_str()),
            ("defence_path", e.defence_path.as_str()),
        ];
        for (name, s) in fields {
            if s.is_empty() {
                empty_fields += 1;
                if verbose {
                    println!("empty {name} for key={:?}", k);
                }
            }
            if let Some(root) = data_root {
                let p = root.join(s);
                if !s.is_empty() && !p.exists() {
                    missing_files += 1;
                    if verbose {
                        println!("missing {name}: {} (key={:?})", p.display(), k);
                    }
                }
            }
        }

        if verbose {
            println!(
                "key={:?} unk_id={} anims={} locs={}",
                k,
                e.unk_id,
                e.animations.len(),
                e.locators.len()
            );
            println!("  model: {}", e.model_path);
            println!("  mmt:   {}", e.material_table_path);
            println!("  cfg:   {}", e.config_path);
            println!("  icon:  {}", e.icon_path);
            println!("  def:   {}", e.defence_path);
        }
    }

    let dupe_keys: Vec<_> = dupes.iter().filter(|(_, v)| **v > 1).collect();
    println!("unique keys: {}", dupes.len());
    println!("duplicate keys: {}", dupe_keys.len());
    if !dupe_keys.is_empty() {
        println!("first dupes:");
        for (k, v) in dupe_keys.into_iter().take(10) {
            println!("  {:?} x{}", k, v);
        }
    }

    println!("animations length histogram:");
    for (k, v) in anim_lens {
        println!("  {k}: {v}");
    }
    println!("locators length histogram:");
    for (k, v) in loc_lens {
        println!("  {k}: {v}");
    }

    println!("empty field count: {empty_fields}");
    if data_root.is_some() {
        println!("missing referenced files: {missing_files}");
        if let Some(r) = data_root {
            println!("data_root: {}", r.display());
        }
    }

    Ok(())
}

fn diff_two(a: &Path, b: &Path) -> anyhow::Result<()> {
    let da = svza::fb::trpmcatalog::read_doc(std::fs::read(a)?)?;
    let db = svza::fb::trpmcatalog::read_doc(std::fs::read(b)?)?;

    let mut ka: BTreeMap<(u16, u16, u8), svza::fb::trpmcatalog::CatalogEntryFull> = BTreeMap::new();
    for e in da.entries {
        ka.insert((e.key.species, e.key.form, e.key.gender), e);
    }
    let mut kb: BTreeMap<(u16, u16, u8), svza::fb::trpmcatalog::CatalogEntryFull> = BTreeMap::new();
    for e in db.entries {
        kb.insert((e.key.species, e.key.form, e.key.gender), e);
    }

    let keys: BTreeSet<_> = ka.keys().chain(kb.keys()).cloned().collect();
    let mut only_a = 0usize;
    let mut only_b = 0usize;
    let mut changed = 0usize;

    for k in &keys {
        match (ka.get(k), kb.get(k)) {
            (Some(_), None) => only_a += 1,
            (None, Some(_)) => only_b += 1,
            (Some(ea), Some(eb)) => {
                if !same_entry(ea, eb) {
                    changed += 1;
                }
            }
            (None, None) => {}
        }
    }

    println!("keys only in A: {only_a}");
    println!("keys only in B: {only_b}");
    println!("keys changed:   {changed}");

    println!("first changes:");
    let mut shown = 0usize;
    for k in keys {
        let (Some(ea), Some(eb)) = (ka.get(&k), kb.get(&k)) else {
            continue;
        };
        if same_entry(ea, eb) {
            continue;
        }
        println!("  key={k:?}");
        if ea.model_path != eb.model_path {
            println!("    model: {}  !=  {}", ea.model_path, eb.model_path);
        }
        if ea.config_path != eb.config_path {
            println!("    cfg:   {}  !=  {}", ea.config_path, eb.config_path);
        }
        if ea.material_table_path != eb.material_table_path {
            println!(
                "    mmt:   {}  !=  {}",
                ea.material_table_path, eb.material_table_path
            );
        }
        if ea.icon_path != eb.icon_path {
            println!("    icon:  {}  !=  {}", ea.icon_path, eb.icon_path);
        }
        if ea.defence_path != eb.defence_path {
            println!("    def:   {}  !=  {}", ea.defence_path, eb.defence_path);
        }
        if ea.unk_id != eb.unk_id {
            println!("    unk:   {}  !=  {}", ea.unk_id, eb.unk_id);
        }
        if ea.animations.len() != eb.animations.len() {
            println!(
                "    anims: {}  !=  {}",
                ea.animations.len(),
                eb.animations.len()
            );
        }
        if ea.locators.len() != eb.locators.len() {
            println!(
                "    locs:  {}  !=  {}",
                ea.locators.len(),
                eb.locators.len()
            );
        }
        shown += 1;
        if shown >= 20 {
            break;
        }
    }

    Ok(())
}

fn same_entry(
    a: &svza::fb::trpmcatalog::CatalogEntryFull,
    b: &svza::fb::trpmcatalog::CatalogEntryFull,
) -> bool {
    a.model_path == b.model_path
        && a.material_table_path == b.material_table_path
        && a.config_path == b.config_path
        && a.icon_path == b.icon_path
        && a.defence_path == b.defence_path
        && a.unk_id == b.unk_id
        && a.animations.len() == b.animations.len()
        && a.locators.len() == b.locators.len()
}
