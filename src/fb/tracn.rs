use crate::fb::raw::FbBuf;

pub fn strip_tralk_filenames_in_place(buf: &mut [u8]) -> anyhow::Result<usize> {
    let fb = FbBuf::new(buf.to_vec());
    let root = fb.root_table_pos()?;
    let root_vt = fb.vtable_pos(root)?;

    let Some(entries) = fb.table_field_vec_of_tables(root, root_vt, 0)? else {
        return Ok(0);
    };

    let mut changed = 0usize;
    for epos in entries {
        let evt = fb.vtable_pos(epos)?;
        let Some((_len_pos, bytes_pos, len)) = fb.table_field_string_loc(epos, evt, 1)? else {
            continue;
        };
        let end = bytes_pos + len;
        if end > buf.len() {
            continue;
        }
        let s = String::from_utf8_lossy(&buf[bytes_pos..end]).to_string();
        if !s.ends_with(".tralk") {
            continue;
        }

        // overwrite bytes with 0 and set len=0
        for b in &mut buf[bytes_pos..end] {
            *b = 0;
        }
        let len_pos = _len_pos;
        if len_pos + 4 <= buf.len() {
            buf[len_pos..len_pos + 4].copy_from_slice(&0u32.to_le_bytes());
        }
        changed += 1;
    }
    Ok(changed)
}
