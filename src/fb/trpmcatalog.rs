use crate::fb::raw::FbBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpeciesKey {
    pub species: u16,
    pub form: u16,
    pub gender: u8,
}

#[derive(Debug, Clone)]
pub struct CatalogEntryLite {
    pub key: SpeciesKey,
    pub model_path: String,
}

#[derive(Debug, Clone)]
pub struct AnimationInfo {
    pub form_number: i16,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct LocatorInfo {
    pub form_number: i16,
    pub loc_index: u8,
    pub loc_path: String,
}

#[derive(Debug, Clone)]
pub struct CatalogEntryFull {
    pub key: SpeciesKey,
    pub model_path: String,
    pub material_table_path: String,
    pub config_path: String,
    pub animations: Vec<AnimationInfo>,
    pub locators: Vec<LocatorInfo>,
    pub icon_path: String,
    pub unk_id: u32,
    pub defence_path: String,
}

#[derive(Debug, Clone)]
pub struct CatalogDoc {
    pub version: u32,
    pub entries: Vec<CatalogEntryFull>,
}

pub fn read_entries(buf: Vec<u8>) -> anyhow::Result<Vec<CatalogEntryLite>> {
    let fb = FbBuf::new(buf);
    let root = fb.root_table_pos()?;
    let root_vt = fb.vtable_pos(root)?;

    let Some(entry_tables) = fb.table_field_vec_of_tables(root, root_vt, 1)? else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(entry_tables.len());
    for tpos in entry_tables {
        let vt = fb.vtable_pos(tpos)?;

        let key = {
            let Some(spec_pos) = fb.table_field_table_pos(tpos, vt, 0)? else {
                continue;
            };
            let spec_vt = fb.vtable_pos(spec_pos)?;
            let species = fb
                .table_field_scalar_u16(spec_pos, spec_vt, 0)?
                .unwrap_or(0);
            let form = fb
                .table_field_scalar_u16(spec_pos, spec_vt, 1)?
                .unwrap_or(0);
            let gender = fb.table_field_scalar_u8(spec_pos, spec_vt, 2)?.unwrap_or(0);
            SpeciesKey {
                species,
                form,
                gender,
            }
        };

        let model_path = fb.table_field_string(tpos, vt, 1)?.unwrap_or_default();
        if model_path.is_empty() {
            continue;
        }

        out.push(CatalogEntryLite { key, model_path });
    }
    Ok(out)
}

pub fn read_doc(buf: Vec<u8>) -> anyhow::Result<CatalogDoc> {
    let fb = FbBuf::new(buf);
    let root = fb.root_table_pos()?;
    let root_vt = fb.vtable_pos(root)?;

    let version = if let Some(vpos) = fb.table_field_table_pos(root, root_vt, 0)? {
        let vvt = fb.vtable_pos(vpos)?;
        fb.table_field_scalar_u32(vpos, vvt, 0)?.unwrap_or(0)
    } else {
        0
    };

    let Some(entry_tables) = fb.table_field_vec_of_tables(root, root_vt, 1)? else {
        return Ok(CatalogDoc {
            version,
            entries: Vec::new(),
        });
    };

    let mut entries = Vec::with_capacity(entry_tables.len());
    for tpos in entry_tables {
        let vt = fb.vtable_pos(tpos)?;

        let key = {
            let Some(spec_pos) = fb.table_field_table_pos(tpos, vt, 0)? else {
                continue;
            };
            let spec_vt = fb.vtable_pos(spec_pos)?;
            let species = fb
                .table_field_scalar_u16(spec_pos, spec_vt, 0)?
                .unwrap_or(0);
            let form = fb
                .table_field_scalar_u16(spec_pos, spec_vt, 1)?
                .unwrap_or(0);
            let gender = fb.table_field_scalar_u8(spec_pos, spec_vt, 2)?.unwrap_or(0);
            SpeciesKey {
                species,
                form,
                gender,
            }
        };

        let model_path = fb.table_field_string(tpos, vt, 1)?.unwrap_or_default();
        let material_table_path = fb.table_field_string(tpos, vt, 2)?.unwrap_or_default();
        let config_path = fb.table_field_string(tpos, vt, 3)?.unwrap_or_default();
        let icon_path = fb.table_field_string(tpos, vt, 6)?.unwrap_or_default();
        let unk_id = fb.table_field_scalar_u32(tpos, vt, 7)?.unwrap_or(0);
        let defence_path = fb.table_field_string(tpos, vt, 8)?.unwrap_or_default();

        let animations = if let Some(anim_tables) = fb.table_field_vec_of_tables(tpos, vt, 4)? {
            let mut v = Vec::with_capacity(anim_tables.len());
            for apos in anim_tables {
                let avt = fb.vtable_pos(apos)?;
                let form_number = fb
                    .table_field_scalar_u16(apos, avt, 0)?
                    .map(|x| x as i16)
                    .unwrap_or(0);
                let path = fb.table_field_string(apos, avt, 1)?.unwrap_or_default();
                v.push(AnimationInfo { form_number, path });
            }
            v
        } else {
            Vec::new()
        };

        let locators = if let Some(loc_tables) = fb.table_field_vec_of_tables(tpos, vt, 5)? {
            let mut v = Vec::with_capacity(loc_tables.len());
            for lpos in loc_tables {
                let lvt = fb.vtable_pos(lpos)?;
                let form_number = fb
                    .table_field_scalar_u16(lpos, lvt, 0)?
                    .map(|x| x as i16)
                    .unwrap_or(0);
                let loc_index = fb.table_field_scalar_u8(lpos, lvt, 1)?.unwrap_or(0);
                let loc_path = fb.table_field_string(lpos, lvt, 2)?.unwrap_or_default();
                v.push(LocatorInfo {
                    form_number,
                    loc_index,
                    loc_path,
                });
            }
            v
        } else {
            Vec::new()
        };

        entries.push(CatalogEntryFull {
            key,
            model_path,
            material_table_path,
            config_path,
            animations,
            locators,
            icon_path,
            unk_id,
            defence_path,
        });
    }

    Ok(CatalogDoc { version, entries })
}

pub fn write_doc(doc: &CatalogDoc) -> anyhow::Result<Vec<u8>> {
    let mut w = Writer::new();
    w.write_catalog(doc)
}

struct Writer {
    b: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { b: Vec::new() }
    }

    fn align(&mut self, n: usize) {
        let pad = (n - (self.b.len() % n)) % n;
        self.b.extend(std::iter::repeat(0).take(pad));
    }

    fn pos(&self) -> usize {
        self.b.len()
    }

    fn put_u8(&mut self, v: u8) {
        self.b.push(v);
    }
    fn put_u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_i16(&mut self, v: i16) {
        self.put_u16(v as u16);
    }
    fn put_u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn put_i32(&mut self, v: i32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }

    fn patch_u32(&mut self, at: usize, v: u32) -> anyhow::Result<()> {
        let end = at
            .checked_add(4)
            .ok_or_else(|| anyhow::anyhow!("patch overflow"))?;
        if end > self.b.len() {
            anyhow::bail!("patch out of bounds: {at}");
        }
        self.b[at..end].copy_from_slice(&v.to_le_bytes());
        Ok(())
    }

    fn write_string(&mut self, s: &str) -> usize {
        self.align(4);
        let start = self.pos();
        self.put_u32(s.len() as u32);
        self.b.extend_from_slice(s.as_bytes());
        self.put_u8(0);
        start
    }

    fn write_table_header(&mut self, vtable_len: usize, obj_align: usize) -> (usize, usize) {
        self.align(2);
        let vtable_pos = self.pos();
        self.b.extend(std::iter::repeat(0).take(vtable_len));
        let pad = (obj_align - (self.pos() % obj_align)) % obj_align;
        self.b.extend(std::iter::repeat(0).take(pad));
        let obj_pos = self.pos();
        (vtable_pos, obj_pos)
    }

    fn write_vtable(
        &mut self,
        vtable_pos: usize,
        vtable_len: u16,
        obj_len: u16,
        field_offsets: &[u16],
    ) -> anyhow::Result<()> {
        let need = vtable_pos + (vtable_len as usize);
        if need > self.b.len() {
            anyhow::bail!("vtable write out of bounds");
        }
        // u16 vtable_len, u16 obj_len, then offsets
        self.b[vtable_pos..vtable_pos + 2].copy_from_slice(&vtable_len.to_le_bytes());
        self.b[vtable_pos + 2..vtable_pos + 4].copy_from_slice(&obj_len.to_le_bytes());
        let mut p = vtable_pos + 4;
        for &o in field_offsets {
            self.b[p..p + 2].copy_from_slice(&o.to_le_bytes());
            p += 2;
        }
        Ok(())
    }

    fn write_species_info(&mut self, key: SpeciesKey) -> anyhow::Result<usize> {
        // fields: species u16 @4, form u16 @6, gender u8 @8
        let field_offsets = [4u16, 6u16, 8u16];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 12u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);
        self.put_u16(key.species);
        self.put_u16(key.form);
        self.put_u8(key.gender);
        self.put_u8(0);
        self.put_u16(0);

        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;
        Ok(obj_pos)
    }

    fn write_animation_info(&mut self, a: &AnimationInfo) -> anyhow::Result<usize> {
        let field_offsets = [4u16, 8u16];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 12u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);
        self.put_i16(a.form_number);
        self.put_u16(0);
        let uoff_pos = self.pos();
        self.put_u32(0);

        let s_pos = self.write_string(&a.path);
        self.patch_u32(uoff_pos, (s_pos - uoff_pos) as u32)?;
        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;
        Ok(obj_pos)
    }

    fn write_locator_info(&mut self, l: &LocatorInfo) -> anyhow::Result<usize> {
        let field_offsets = [4u16, 6u16, 8u16];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 12u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);
        self.put_i16(l.form_number);
        self.put_u8(l.loc_index);
        self.put_u8(0);
        let uoff_pos = self.pos();
        self.put_u32(0);

        let s_pos = self.write_string(&l.loc_path);
        self.patch_u32(uoff_pos, (s_pos - uoff_pos) as u32)?;
        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;
        Ok(obj_pos)
    }

    fn write_vec_of_tables<F>(&mut self, n: usize, mut write_elem: F) -> anyhow::Result<usize>
    where
        F: FnMut(&mut Self, usize, usize) -> anyhow::Result<usize>,
    {
        self.align(4);
        let vec_pos = self.pos();
        self.put_u32(n as u32);
        let mut uoff_positions = Vec::with_capacity(n);
        for _ in 0..n {
            uoff_positions.push(self.pos());
            self.put_u32(0);
        }
        for (i, uoff_pos) in uoff_positions.into_iter().enumerate() {
            let elem_pos = write_elem(self, i, uoff_pos)?;
            self.patch_u32(uoff_pos, (elem_pos - uoff_pos) as u32)?;
        }
        Ok(vec_pos)
    }

    fn write_catalog_entry(&mut self, e: &CatalogEntryFull) -> anyhow::Result<usize> {
        // 9 fields, all 4-byte except unk_id which is u32 as well
        let field_offsets = [4u16, 8, 12, 16, 20, 24, 28, 32, 36];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 40u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);

        let u_species = self.pos();
        self.put_u32(0);
        let u_model = self.pos();
        self.put_u32(0);
        let u_trmmt = self.pos();
        self.put_u32(0);
        let u_cfg = self.pos();
        self.put_u32(0);
        let u_anims = self.pos();
        self.put_u32(0);
        let u_locs = self.pos();
        self.put_u32(0);
        let u_icon = self.pos();
        self.put_u32(0);
        self.put_u32(e.unk_id);
        let u_def = self.pos();
        self.put_u32(0);

        let species_pos = self.write_species_info(e.key)?;
        self.patch_u32(u_species, (species_pos - u_species) as u32)?;

        let s_model = self.write_string(&e.model_path);
        self.patch_u32(u_model, (s_model - u_model) as u32)?;
        let s_trmmt = self.write_string(&e.material_table_path);
        self.patch_u32(u_trmmt, (s_trmmt - u_trmmt) as u32)?;
        let s_cfg = self.write_string(&e.config_path);
        self.patch_u32(u_cfg, (s_cfg - u_cfg) as u32)?;

        let anim_vec_pos = self.write_vec_of_tables(e.animations.len(), |w, i, _| {
            w.write_animation_info(&e.animations[i])
        })?;
        self.patch_u32(u_anims, (anim_vec_pos - u_anims) as u32)?;

        let loc_vec_pos = self.write_vec_of_tables(e.locators.len(), |w, i, _| {
            w.write_locator_info(&e.locators[i])
        })?;
        self.patch_u32(u_locs, (loc_vec_pos - u_locs) as u32)?;

        let s_icon = self.write_string(&e.icon_path);
        self.patch_u32(u_icon, (s_icon - u_icon) as u32)?;

        let s_def = self.write_string(&e.defence_path);
        self.patch_u32(u_def, (s_def - u_def) as u32)?;

        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;
        Ok(obj_pos)
    }

    fn write_version_info(&mut self, version: u32) -> anyhow::Result<usize> {
        let field_offsets = [4u16];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 8u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);
        self.put_u32(version);
        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;
        Ok(obj_pos)
    }

    fn write_catalog(&mut self, doc: &CatalogDoc) -> anyhow::Result<Vec<u8>> {
        self.align(4);
        let root_uoff_pos = self.pos();
        self.put_u32(0);

        // root table fields: version(table) @4, table(vec) @8
        let field_offsets = [4u16, 8u16];
        let vtable_len = (4 + field_offsets.len() * 2) as u16;
        let obj_len = 12u16;
        let (vt_pos, obj_pos) = self.write_table_header(vtable_len as usize, 4);

        let vt_dist = (obj_pos - vt_pos) as i32;
        self.put_i32(vt_dist);
        let u_version = self.pos();
        self.put_u32(0);
        let u_table = self.pos();
        self.put_u32(0);

        self.write_vtable(vt_pos, vtable_len, obj_len, &field_offsets)?;

        // patch root offset
        self.patch_u32(root_uoff_pos, (obj_pos - root_uoff_pos) as u32)?;

        let vpos = self.write_version_info(doc.version)?;
        self.patch_u32(u_version, (vpos - u_version) as u32)?;

        let vec_pos = self.write_vec_of_tables(doc.entries.len(), |w, i, _| {
            w.write_catalog_entry(&doc.entries[i])
        })?;
        self.patch_u32(u_table, (vec_pos - u_table) as u32)?;

        Ok(std::mem::take(&mut self.b))
    }
}
