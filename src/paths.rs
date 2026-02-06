use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvLayout {
    Pokemon,
    IkPokemon,
}

pub fn canonicalish(path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        return PathBuf::new();
    }
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn detect_sv_layout(sv_root: &Path) -> Option<(SvLayout, PathBuf)> {
    let ik = sv_root.join("ik_pokemon");
    if ik.is_dir() {
        return Some((SvLayout::IkPokemon, ik));
    }
    let p = sv_root.join("pokemon");
    if p.is_dir() {
        return Some((SvLayout::Pokemon, p));
    }
    None
}

pub fn find_under(root: &Path, rel: &str, file_name: &str) -> anyhow::Result<PathBuf> {
    let candidate = root.join(rel);
    if candidate.exists() {
        return Ok(candidate);
    }

    let mut matches = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy() == file_name {
            matches.push(entry.path().to_path_buf());
        }
    }

    if matches.is_empty() {
        anyhow::bail!("could not find {file_name} under {root:?} (expected {candidate:?})");
    }

    matches.sort_by_key(|p| (p.to_string_lossy().len(), p.to_string_lossy().to_string()));
    Ok(matches[0].clone())
}
