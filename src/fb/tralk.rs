use crate::fb::raw::FbBuf;

pub fn patch_no_head_joint_rotation_in_place(buf: &mut [u8]) -> anyhow::Result<usize> {
    let fb = FbBuf::new(buf.to_vec());
    let root = fb.root_table_pos()?;
    let root_vt = fb.vtable_pos(root)?;

    let Some(groups) = fb.table_field_vec_of_tables(root, root_vt, 4)? else {
        return Ok(0);
    };

    let mut changed = 0usize;
    for gpos in groups {
        let gvt = fb.vtable_pos(gpos)?;

        let name = fb.table_field_string(gpos, gvt, 0)?.unwrap_or_default();
        if name != "head" {
            continue;
        }
        let look_at_type = fb.table_field_scalar_u32(gpos, gvt, 4)?.unwrap_or(0);
        if look_at_type != 0 {
            continue;
        }

        // a3 rotationWeights -> [0.0, 0.0]
        if let Some(vec_pos) = fb.table_field_vec_pos(gpos, gvt, 23)? {
            if vec_pos + 4 <= buf.len() {
                let Ok(hdr) = <[u8; 4]>::try_from(&buf[vec_pos..vec_pos + 4]) else {
                    continue;
                };
                let n = u32::from_le_bytes(hdr) as usize;
                for i in 0..n {
                    let at = vec_pos + 4 + i * 4;
                    if at + 4 > buf.len() {
                        break;
                    }
                    write_f32(buf, at, 0.0)?;
                }
            }
        }

        for field in 8..=13 {
            if let Some(loc) = fb.table_field_loc(gpos, gvt, field)? {
                write_f32(buf, loc, 0.001)?;
            }
        }

        // b2 enableTurningClamp -> false
        if let Some(loc) = fb.table_field_loc(gpos, gvt, 15)? {
            if loc < buf.len() {
                buf[loc] = 0;
            }
        }

        changed += 1;
    }

    Ok(changed)
}

fn write_f32(buf: &mut [u8], pos: usize, v: f32) -> anyhow::Result<()> {
    if pos + 4 > buf.len() {
        anyhow::bail!("fb: out of bounds write_f32 at {pos}");
    }
    buf[pos..pos + 4].copy_from_slice(&v.to_bits().to_le_bytes());
    Ok(())
}
