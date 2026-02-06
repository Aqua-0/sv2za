use crate::backend::textures::bntx::{build_index, BntxIndexDoc};
use crate::progress::ProgressSink;
use std::{fs, path::Path, path::PathBuf};

pub fn load_or_build_index(
    za_dump: &Path,
    cache_path: &Path,
    progress: &ProgressSink,
) -> anyhow::Result<BntxIndexDoc> {
    if cache_path.is_file() {
        let doc: BntxIndexDoc = serde_json::from_slice(&fs::read(cache_path)?)?;
        progress.info(format!(
            "[tex] loaded bntx index: {:?} (entries={})",
            cache_path,
            doc.entries.len()
        ));
        return Ok(doc);
    }
    progress.info(format!("[tex] building bntx index: {:?}", cache_path));
    let doc = build_index(za_dump)?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache_path, serde_json::to_vec_pretty(&doc)?)?;
    progress.info(format!(
        "[tex] wrote bntx index: {:?} (entries={})",
        cache_path,
        doc.entries.len()
    ));
    Ok(doc)
}

pub fn default_cache_path(out_root: &Path) -> PathBuf {
    out_root.join("_cache").join("bntx_index_za.json")
}
