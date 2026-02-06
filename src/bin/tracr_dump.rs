use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;
use serde::Serialize;

#[derive(Debug, Parser)]
struct Args {
    /// Input `*_base.tracr` path
    #[arg(required = true)]
    tracr: PathBuf,

    /// Output JSON path (defaults to stdout)
    #[arg(long)]
    out: Option<PathBuf>,

    /// Only emit tracks whose name contains this substring (case-insensitive)
    #[arg(long)]
    filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TracrDoc {
    track_count: usize,
    turn_group_count: usize,
    tracks: Vec<TrackDoc>,
    turn_groups: Vec<TurnGroupDoc>,
}

#[derive(Debug, Clone, Serialize)]
struct TrackDoc {
    track_name: String,
    files: TrackFiles,
    res_0: Option<u32>,
    res_1: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct TrackFiles {
    animation: String,
    material: String,
    effect: String,
    curve: String,
}

#[derive(Debug, Clone, Serialize)]
struct TurnGroupDoc {
    name: String,
    base_name: String,
    flags: u32,
    entries: Vec<TurnEntryDoc>,
}

#[derive(Debug, Clone, Serialize)]
struct TurnEntryDoc {
    filename: String,
    weight: f32,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let b = std::fs::read(&args.tracr).with_context(|| format!("read {}", args.tracr.display()))?;
    let mut doc = read_tracr(b).context("parse TRACR")?;

    if let Some(f) = args
        .filter
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let f_l = f.to_ascii_lowercase();
        doc.tracks
            .retain(|t| t.track_name.to_ascii_lowercase().contains(&f_l));
        doc.track_count = doc.tracks.len();
    }

    let text = serde_json::to_string_pretty(&doc)? + "\n";
    if let Some(out) = &args.out {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, text)?;
    } else {
        print!("{text}");
    }
    Ok(())
}

fn read_tracr(buf: Vec<u8>) -> anyhow::Result<TracrDoc> {
    let fb = svza::fb::raw::FbBuf::new(buf);
    let root = fb.root_table_pos()?;
    let root_vt = fb.vtable_pos(root)?;

    let Some(track_list_pos) = fb.table_field_table_pos(root, root_vt, 0)? else {
        return Ok(TracrDoc {
            track_count: 0,
            turn_group_count: 0,
            tracks: Vec::new(),
            turn_groups: Vec::new(),
        });
    };
    let tl_vt = fb.vtable_pos(track_list_pos)?;

    let tracks_pos = fb
        .table_field_vec_of_tables(track_list_pos, tl_vt, 0)?
        .unwrap_or_default();
    let turn_groups_pos = fb
        .table_field_vec_of_tables(track_list_pos, tl_vt, 1)?
        .unwrap_or_default();

    let mut tracks = Vec::with_capacity(tracks_pos.len());
    for tpos in tracks_pos {
        let vt = fb.vtable_pos(tpos)?;
        let track_name = fb.table_field_string(tpos, vt, 0)?.unwrap_or_default();
        let res_0 = fb.table_field_scalar_u32(tpos, vt, 1)?;
        let res_1 = fb.table_field_scalar_u32(tpos, vt, 2)?;

        let mut files = TrackFiles::default();
        if let Some(tr_pos) = fb.table_field_table_pos(tpos, vt, 3)? {
            let tr_vt = fb.vtable_pos(tr_pos)?;
            files.animation = read_filename(&fb, tr_pos, tr_vt, 0)?;
            files.material = read_filename(&fb, tr_pos, tr_vt, 1)?;
            files.effect = read_filename(&fb, tr_pos, tr_vt, 2)?;
            files.curve = read_filename(&fb, tr_pos, tr_vt, 3)?;
        }

        tracks.push(TrackDoc {
            track_name,
            files,
            res_0,
            res_1,
        });
    }

    let mut turn_groups = Vec::with_capacity(turn_groups_pos.len());
    for gpos in turn_groups_pos {
        let gvt = fb.vtable_pos(gpos)?;
        let name = fb.table_field_string(gpos, gvt, 0)?.unwrap_or_default();

        let (base_name, flags) = if let Some(bpos) = fb.table_field_table_pos(gpos, gvt, 1)? {
            let bvt = fb.vtable_pos(bpos)?;
            let bn = fb.table_field_string(bpos, bvt, 0)?.unwrap_or_default();
            let fl = fb.table_field_scalar_u32(bpos, bvt, 1)?.unwrap_or(0);
            (bn, fl)
        } else {
            (String::new(), 0)
        };

        let mut entries: Vec<TurnEntryDoc> = Vec::new();
        if let Some(epos_list) = fb.table_field_vec_of_tables(gpos, gvt, 2)? {
            entries.reserve(epos_list.len());
            for epos in epos_list {
                let evt = fb.vtable_pos(epos)?;
                let filename = if let Some(tnpos) = fb.table_field_table_pos(epos, evt, 0)? {
                    let tnvt = fb.vtable_pos(tnpos)?;
                    fb.table_field_string(tnpos, tnvt, 0)?.unwrap_or_default()
                } else {
                    String::new()
                };

                let weight = if let Some(loc) = fb.table_field_loc(epos, evt, 1)? {
                    let bits = fb.read_u32(loc)?;
                    f32::from_bits(bits)
                } else {
                    0.0
                };

                entries.push(TurnEntryDoc { filename, weight });
            }
        }

        turn_groups.push(TurnGroupDoc {
            name,
            base_name,
            flags,
            entries,
        });
    }

    Ok(TracrDoc {
        track_count: tracks.len(),
        turn_group_count: turn_groups.len(),
        tracks,
        turn_groups,
    })
}

fn read_filename(
    fb: &svza::fb::raw::FbBuf,
    parent_table_pos: usize,
    parent_vt: usize,
    field_index: usize,
) -> anyhow::Result<String> {
    let Some(res_pos) = fb.table_field_table_pos(parent_table_pos, parent_vt, field_index)? else {
        return Ok(String::new());
    };
    let res_vt = fb.vtable_pos(res_pos)?;
    Ok(fb
        .table_field_string(res_pos, res_vt, 0)?
        .unwrap_or_default())
}
