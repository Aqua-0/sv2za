#[derive(Clone)]
pub struct FbBuf {
    b: Vec<u8>,
}

impl FbBuf {
    pub fn new(b: Vec<u8>) -> Self {
        Self { b }
    }

    pub fn read_u8(&self, pos: usize) -> anyhow::Result<u8> {
        self.b
            .get(pos)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("fb: out of bounds u8 at {pos}"))
    }

    pub fn read_u16(&self, pos: usize) -> anyhow::Result<u16> {
        let end = pos
            .checked_add(2)
            .ok_or_else(|| anyhow::anyhow!("fb: u16 overflow"))?;
        let s = self
            .b
            .get(pos..end)
            .ok_or_else(|| anyhow::anyhow!("fb: out of bounds u16 at {pos}"))?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    pub fn read_u32(&self, pos: usize) -> anyhow::Result<u32> {
        let end = pos
            .checked_add(4)
            .ok_or_else(|| anyhow::anyhow!("fb: u32 overflow"))?;
        let s = self
            .b
            .get(pos..end)
            .ok_or_else(|| anyhow::anyhow!("fb: out of bounds u32 at {pos}"))?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    pub fn read_i32(&self, pos: usize) -> anyhow::Result<i32> {
        Ok(self.read_u32(pos)? as i32)
    }

    pub fn root_table_pos(&self) -> anyhow::Result<usize> {
        let off = self.read_u32(0)? as usize;
        if off >= self.b.len() {
            anyhow::bail!("fb: root table offset out of bounds: {off}");
        }
        Ok(off)
    }

    pub fn vtable_pos(&self, table_pos: usize) -> anyhow::Result<usize> {
        // In FlatBuffers this is a signed offset from the table start to the vtable
        // (usually negative, since vtables are stored before tables)
        let soff = self.read_i32(table_pos)? as isize;
        let vt = (table_pos as isize)
            .checked_sub(soff)
            .ok_or_else(|| anyhow::anyhow!("fb: vtable pos overflow"))?;
        if vt < 0 {
            anyhow::bail!("fb: vtable pos underflow at {table_pos} (soff={soff})");
        }
        Ok(vt as usize)
    }

    pub fn field_offset(&self, vtable_pos: usize, field_index: usize) -> anyhow::Result<u16> {
        let vtable_len = self.read_u16(vtable_pos)? as usize;
        let want = 4usize + field_index * 2;
        if want + 2 > vtable_len {
            return Ok(0);
        }
        self.read_u16(vtable_pos + want)
    }

    pub fn table_field_loc(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<usize>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        Ok(Some(table_pos + fo))
    }

    pub fn table_field_table_pos(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<usize>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        let at = table_pos + fo;
        let uoff = self.read_u32(at)? as usize;
        if uoff == 0 {
            return Ok(None);
        }
        let target = at
            .checked_add(uoff)
            .ok_or_else(|| anyhow::anyhow!("fb: uoff overflow"))?;
        if target >= self.b.len() {
            anyhow::bail!("fb: table field target out of bounds: {target}");
        }
        Ok(Some(target))
    }

    pub fn table_field_string(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<String>> {
        let Some((_len_pos, bytes_pos, len)) =
            self.table_field_string_loc(table_pos, vtable_pos, field_index)?
        else {
            return Ok(None);
        };
        let end = bytes_pos
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("fb: str end overflow"))?;
        let bytes = self
            .b
            .get(bytes_pos..end)
            .ok_or_else(|| anyhow::anyhow!("fb: out of bounds string bytes"))?;
        Ok(Some(String::from_utf8_lossy(bytes).to_string()))
    }

    pub fn table_field_string_loc(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<(usize, usize, usize)>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        let at = table_pos + fo;
        let uoff = self.read_u32(at)? as usize;
        if uoff == 0 {
            return Ok(None);
        }
        let str_pos = at
            .checked_add(uoff)
            .ok_or_else(|| anyhow::anyhow!("fb: str uoff overflow"))?;
        let len = self.read_u32(str_pos)? as usize;
        let bytes_pos = str_pos
            .checked_add(4)
            .ok_or_else(|| anyhow::anyhow!("fb: str bytes_pos overflow"))?;
        Ok(Some((str_pos, bytes_pos, len)))
    }

    pub fn table_field_scalar_u32(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<u32>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        Ok(Some(self.read_u32(table_pos + fo)?))
    }

    pub fn table_field_scalar_u16(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<u16>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        Ok(Some(self.read_u16(table_pos + fo)?))
    }

    pub fn table_field_scalar_u8(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<u8>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        Ok(Some(self.read_u8(table_pos + fo)?))
    }

    pub fn table_field_vec_of_tables(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<Vec<usize>>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        let at = table_pos + fo;
        let uoff = self.read_u32(at)? as usize;
        if uoff == 0 {
            return Ok(None);
        }
        let vec_pos = at
            .checked_add(uoff)
            .ok_or_else(|| anyhow::anyhow!("fb: vec uoff overflow"))?;
        let n = self.read_u32(vec_pos)? as usize;
        let mut out = Vec::with_capacity(n);
        let base = vec_pos
            .checked_add(4)
            .ok_or_else(|| anyhow::anyhow!("fb: vec base overflow"))?;
        for i in 0..n {
            let elem_pos = base
                .checked_add(i * 4)
                .ok_or_else(|| anyhow::anyhow!("fb: vec elem overflow"))?;
            let uoff = self.read_u32(elem_pos)? as usize;
            if uoff == 0 {
                continue;
            }
            let tpos = elem_pos
                .checked_add(uoff)
                .ok_or_else(|| anyhow::anyhow!("fb: table elem uoff overflow"))?;
            if tpos >= self.b.len() {
                anyhow::bail!("fb: vec elem table out of bounds: {tpos}");
            }
            out.push(tpos);
        }
        Ok(Some(out))
    }

    pub fn table_field_vec_pos(
        &self,
        table_pos: usize,
        vtable_pos: usize,
        field_index: usize,
    ) -> anyhow::Result<Option<usize>> {
        let fo = self.field_offset(vtable_pos, field_index)? as usize;
        if fo == 0 {
            return Ok(None);
        }
        let at = table_pos + fo;
        let uoff = self.read_u32(at)? as usize;
        if uoff == 0 {
            return Ok(None);
        }
        let vec_pos = at
            .checked_add(uoff)
            .ok_or_else(|| anyhow::anyhow!("fb: vec uoff overflow"))?;
        Ok(Some(vec_pos))
    }

    // keep this small; add helpers as we need them
}
