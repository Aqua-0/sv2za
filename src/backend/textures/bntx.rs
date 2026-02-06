use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone)]
pub struct BntxMeta {
    pub width: i32,
    pub height: i32,
    pub mip_count: u16,
    pub data_length: i32,
    pub base_offset: i64,
    pub format_type: u8,
    pub format_var: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BntxIndexEntry {
    pub file_path: String,
    pub file_name: String,
    pub width: i32,
    pub height: i32,
    pub mip_count: i32,
    pub data_length: i32,
    pub base_offset: i64,
    pub ultimate_format: Option<String>,
    pub no_mipmaps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BntxIndexDoc {
    pub dump_root: String,
    pub count_files: usize,
    pub count_entries: usize,
    pub skipped_files: usize,
    pub entries: Vec<BntxIndexEntry>,
    pub by_key: HashMap<String, Vec<usize>>,
    pub by_name: HashMap<String, Vec<usize>>,
}

pub fn read_bntx_metas(path: &Path) -> anyhow::Result<Vec<BntxMeta>> {
    let b = fs::read(path)?;
    if b.len() < 0x40 {
        anyhow::bail!("too small");
    }
    let sig = read_cstr_fixed(&b, 0, 8);
    if sig != "BNTX" {
        anyhow::bail!("bad signature: {sig:?}");
    }

    let mut off = 0x20usize;
    let nx = read_cstr_fixed(&b, off, 4);
    if nx != "NX  " {
        anyhow::bail!("bad NX signature: {nx:?}");
    }
    off += 4;
    let tex_count = read_u32le(&b, off) as usize;
    off += 4;
    let info_ptrs_addr = read_i64le(&b, off);
    off += 8;
    let _data_blk_addr = read_i64le(&b, off);
    off += 8;
    let _dict_addr = read_i64le(&b, off);
    off += 8;
    let _str_dict_len = read_u32le(&b, off);
    let _ = off;

    let mut out = Vec::new();
    for idx in 0..tex_count {
        let ptr_off = info_ptrs_addr + (idx as i64) * 8;
        if ptr_off < 0 || (ptr_off as usize) + 8 > b.len() {
            continue;
        }
        let brti_addr = read_i64le(&b, ptr_off as usize);
        if brti_addr < 0 || (brti_addr as usize) + 4 > b.len() {
            continue;
        }
        if read_cstr_fixed(&b, brti_addr as usize, 4) != "BRTI" {
            continue;
        }

        let mut cur = brti_addr as usize + 4;
        cur += 4;
        cur += 8;
        cur += 1;
        cur += 1;
        cur += 2;
        cur += 2;
        let mip_count = read_u16le(&b, cur);
        cur += 2;
        cur += 2;
        cur += 2;
        let fmt_u32 = read_u32le(&b, cur);
        cur += 4;
        cur += 4;
        let width = read_i32le(&b, cur);
        cur += 4;
        let height = read_i32le(&b, cur);
        cur += 4;
        cur += 4;
        cur += 4;
        let _block_h_log2 = read_i32le(&b, cur);
        cur += 4;
        cur += 4 * 6;
        let data_len = read_i32le(&b, cur);
        cur += 4;
        cur += 4;
        cur += 4;
        cur += 4;
        let name_addr = read_i64le(&b, cur);
        cur += 8;
        cur += 8;
        let ptrs_addr = read_i64le(&b, cur);
        cur += 8;
        let _ = cur;

        if name_addr <= 0 || name_addr as usize >= b.len() {
            continue;
        }
        let _ = read_short_string(&b, name_addr as usize)?;

        if ptrs_addr <= 0 || (ptrs_addr as usize) + 8 > b.len() {
            continue;
        }
        let base_off = read_i64le(&b, ptrs_addr as usize);

        let fmt_type = ((fmt_u32 >> 8) & 0xFF) as u8;
        let fmt_var = ((fmt_u32 >> 0) & 0xFF) as u8;
        out.push(BntxMeta {
            width,
            height,
            mip_count,
            data_length: data_len,
            base_offset: base_off,
            format_type: fmt_type,
            format_var: fmt_var,
        });
    }
    Ok(out)
}

pub fn ultimate_format(format_type: u8, format_var: u8) -> Option<&'static str> {
    match format_type {
        0x1A => Some(if format_var == 6 {
            "BC1RgbaUnormSrgb"
        } else {
            "BC1RgbaUnorm"
        }),
        0x1B => Some(if format_var == 6 {
            "BC2RgbaUnormSrgb"
        } else {
            "BC2RgbaUnorm"
        }),
        0x1C => Some(if format_var == 6 {
            "BC3RgbaUnormSrgb"
        } else {
            "BC3RgbaUnorm"
        }),
        0x1D => Some(if format_var == 2 {
            "BC4RSnorm"
        } else {
            "BC4RUnorm"
        }),
        0x1E => Some(if format_var == 2 {
            "BC5RgSnorm"
        } else {
            "BC5RgUnorm"
        }),
        0x1F => Some("BC6hRgbUfloat"),
        0x20 => Some(if format_var == 6 {
            "BC7RgbaUnormSrgb"
        } else {
            "BC7RgbaUnorm"
        }),
        0x0B => Some(if format_var == 6 {
            "Rgba8UnormSrgb"
        } else {
            "Rgba8Unorm"
        }),
        _ => None,
    }
}

pub fn extract_tex_data(bntx_path: &Path) -> anyhow::Result<(Vec<u8>, usize, usize)> {
    let metas = read_bntx_metas(bntx_path)?;
    let first = metas
        .first()
        .ok_or_else(|| anyhow::anyhow!("no textures in bntx"))?;
    let dlen = first.data_length;
    let boff = first.base_offset;
    let b = fs::read(bntx_path)?;
    if boff < 0 || dlen < 0 {
        anyhow::bail!("invalid data region");
    }
    let boff = boff as usize;
    let dlen = dlen as usize;
    let end = boff
        .checked_add(dlen)
        .ok_or_else(|| anyhow::anyhow!("overflow"))?;
    if end > b.len() {
        anyhow::bail!("invalid data region");
    }
    Ok((b[boff..end].to_vec(), boff, dlen))
}

pub fn build_index(dump_root: &Path) -> anyhow::Result<BntxIndexDoc> {
    let mut files = Vec::new();
    for e in walkdir::WalkDir::new(dump_root).follow_links(false) {
        let e = e?;
        if !e.file_type().is_file() {
            continue;
        }
        if e.path().extension().and_then(|x| x.to_str()) == Some("bntx") {
            files.push(e.path().to_path_buf());
        }
    }
    files.sort();

    let mut entries = Vec::<BntxIndexEntry>::new();
    let mut skipped = 0usize;
    for f in &files {
        let texs = match read_bntx_metas(f) {
            Ok(v) => v,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if texs.is_empty() {
            skipped += 1;
            continue;
        }
        for t in texs {
            let ult = ultimate_format(t.format_type, t.format_var).map(|s| s.to_string());
            entries.push(BntxIndexEntry {
                file_path: f.to_string_lossy().to_string(),
                file_name: f
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                width: t.width,
                height: t.height,
                mip_count: t.mip_count as i32,
                data_length: t.data_length,
                base_offset: t.base_offset,
                ultimate_format: ult,
                no_mipmaps: t.mip_count <= 1,
            });
        }
    }

    let mut by_key = HashMap::<String, Vec<usize>>::new();
    let mut by_name = HashMap::<String, Vec<usize>>::new();
    for (i, m) in entries.iter().enumerate() {
        let k = format!(
            "{}x{}|{}|noMip={}",
            m.width,
            m.height,
            m.ultimate_format
                .clone()
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            if m.no_mipmaps { 1 } else { 0 }
        );
        by_key.entry(k).or_default().push(i);
        by_name
            .entry(m.file_name.to_lowercase())
            .or_default()
            .push(i);
    }

    Ok(BntxIndexDoc {
        dump_root: dump_root.to_string_lossy().to_string(),
        count_files: files.len(),
        count_entries: entries.len(),
        skipped_files: skipped,
        entries,
        by_key,
        by_name,
    })
}

fn read_u16le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(b[off..off + 2].try_into().unwrap())
}
fn read_u32le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn read_i32le(b: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}
fn read_i64le(b: &[u8], off: usize) -> i64 {
    i64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

fn read_cstr_fixed(b: &[u8], off: usize, len: usize) -> String {
    let mut raw = &b[off..off + len];
    if let Some(i) = raw.iter().position(|&x| x == 0) {
        raw = &raw[..i];
    }
    String::from_utf8_lossy(raw).to_string()
}

fn read_short_string(b: &[u8], off: usize) -> anyhow::Result<(String, usize)> {
    if off + 2 > b.len() {
        anyhow::bail!("short string truncated");
    }
    let ln = read_u16le(b, off) as usize;
    let off2 = off + 2;
    let end = off2 + ln;
    if end > b.len() {
        anyhow::bail!("short string truncated");
    }
    let mut raw = &b[off2..end];
    if let Some(i) = raw.iter().position(|&x| x == 0) {
        raw = &raw[..i];
    }
    Ok((String::from_utf8_lossy(raw).to_string(), end))
}
