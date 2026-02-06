use crate::progress::ProgressSink;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct ConvertedMon {
    pub species: u16,
    pub form: u16,
    pub gender: u8,
    pub name: String,
    pub pm: String,
    pub pm_variant: String,
}

pub fn write_converted_names_report(
    za_dump: &Path,
    out_root: &Path,
    mons: &[ConvertedMon],
    lang: &str,
    progress: &ProgressSink,
) -> anyhow::Result<PathBuf> {
    progress.phase_start("Names report");
    let name_map = load_monsname_map(za_dump, lang).unwrap_or_default();

    let mut out = Vec::with_capacity(mons.len());
    for m in mons {
        let mut m2 = m.clone();
        m2.name = name_map.get(&m.species).cloned().unwrap_or_default();
        out.push(m2);
    }
    out.sort_by_key(|m| (m.species, m.form, m.gender, m.pm_variant.clone()));

    let report_dir = out_root.join("_report");
    fs::create_dir_all(&report_dir)?;
    let path = report_dir.join("converted_pokemon.json");
    fs::write(&path, serde_json::to_vec_pretty(&out)?)?;
    progress.info(format!(
        "[names] wrote {:?} (missing_names={})",
        path,
        out.iter().filter(|m| m.name.is_empty()).count()
    ));
    progress.phase_end("Names report");
    Ok(path)
}

pub fn load_monsname_map(
    dump_root: &Path,
    language: &str,
) -> anyhow::Result<BTreeMap<u16, String>> {
    let mut tried = Vec::new();
    for lang in candidate_langs(language) {
        let base = dump_root
            .join("ik_message")
            .join("dat")
            .join(&lang)
            .join("common");
        let tbl = base.join("monsname.tbl");
        let dat = base.join("monsname.dat");
        tried.push((lang, tbl.clone(), dat.clone()));
        if tbl.is_file() && dat.is_file() {
            return load_monsname_map_exact(&tbl, &dat);
        }
    }
    let _ = tried;
    Ok(BTreeMap::new())
}

fn candidate_langs(language: &str) -> Vec<String> {
    let l = language.trim();
    let mut out = Vec::new();
    if !l.is_empty() {
        out.push(l.to_string());
    }
    for s in ["English", "en"] {
        if !out.iter().any(|x| x.eq_ignore_ascii_case(s)) {
            out.push(s.to_string());
        }
    }
    out
}

fn load_monsname_map_exact(tbl: &Path, dat: &Path) -> anyhow::Result<BTreeMap<u16, String>> {
    let keys = read_ahtb_keys(&tbl)?;
    let strings = decode_dat_strings(&dat)?;
    let mut out = BTreeMap::new();
    for (i, k) in keys.iter().enumerate() {
        if k == "msg_monsname_max" {
            continue;
        }
        if !k.starts_with("MONSNAME_") {
            continue;
        }
        let sid = k.split_once('_').and_then(|(_, n)| n.parse::<u16>().ok());
        let Some(sid) = sid else { continue };
        if i < strings.len() {
            out.insert(sid, strings[i].clone());
        }
    }
    Ok(out)
}

fn read_ahtb_keys(path: &Path) -> anyhow::Result<Vec<String>> {
    let b = fs::read(path)?;
    if b.get(0..4) != Some(b"AHTB") {
        anyhow::bail!("not AHTB: {path:?}");
    }
    let count = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
    let mut off = 8usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        off += 8; // hash
        if off + 2 > b.len() {
            anyhow::bail!("AHTB truncated: {path:?}");
        }
        let slen = u16::from_le_bytes(b[off..off + 2].try_into().unwrap()) as usize;
        off += 2;
        let end = off + slen;
        if end > b.len() {
            anyhow::bail!("AHTB truncated: {path:?}");
        }
        let raw = &b[off..end];
        if raw.last().copied() != Some(0) {
            anyhow::bail!("bad AHTB string terminator: {path:?}");
        }
        out.push(String::from_utf8_lossy(&raw[..raw.len() - 1]).to_string());
        off = end;
    }
    Ok(out)
}

fn crypt_utf16_codes(codes: &[u16], str_id: u16) -> Vec<u16> {
    let mut mask = (0x2983u32 * ((str_id as u32 & 0xFFFF) + 3)) & 0xFFFF;
    let mut out = Vec::with_capacity(codes.len());
    for &code in codes {
        out.push(((code as u32 ^ mask) & 0xFFFF) as u16);
        mask = (((mask & 0xE000) >> 13) | ((mask & 0x1FFF) << 3)) & 0xFFFF;
    }
    out
}

fn decode_dat_strings(dat_path: &Path) -> anyhow::Result<Vec<String>> {
    let b = fs::read(dat_path)?;
    if b.len() < 16 {
        return Ok(Vec::new());
    }
    let num_langs = u16::from_le_bytes(b[0..2].try_into().unwrap());
    let num_strings = u16::from_le_bytes(b[2..4].try_into().unwrap()) as usize;
    if num_langs != 1 {
        anyhow::bail!("only supports num_langs=1 for now: {dat_path:?} has {num_langs}");
    }
    let lang0 = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
    let params_off = lang0 + 4;

    let mut out = Vec::with_capacity(num_strings);
    for str_id in 0..num_strings {
        let p = params_off + str_id * 8;
        if p + 8 > b.len() {
            break;
        }
        let ofs = u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        let ln = u16::from_le_bytes(b[p + 4..p + 6].try_into().unwrap()) as usize;
        let start = lang0 + ofs;
        let end = start + ln * 2;
        if end > b.len() {
            out.push(String::new());
            continue;
        }
        let mut codes = Vec::with_capacity(ln);
        for i in 0..ln {
            let at = start + i * 2;
            let c = u16::from_le_bytes(b[at..at + 2].try_into().unwrap());
            codes.push(c);
        }
        let dec = crypt_utf16_codes(&codes, str_id as u16);
        let dec = match dec.iter().position(|&x| x == 0) {
            Some(i) => &dec[..i],
            None => &dec[..],
        };
        let s = String::from_utf16_lossy(dec);
        out.push(s);
    }
    Ok(out)
}
