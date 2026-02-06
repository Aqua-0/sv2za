use std::{fs, path::Path};

pub fn read_bmp_rgba(path: &Path) -> anyhow::Result<(i32, i32, Vec<u8>)> {
    let b = fs::read(path)?;
    if b.len() < 54 || &b[0..2] != b"BM" {
        anyhow::bail!("not a BMP");
    }
    let pixel_off = u32::from_le_bytes(b[10..14].try_into().unwrap()) as usize;
    let dib_size = u32::from_le_bytes(b[14..18].try_into().unwrap());
    if dib_size < 40 {
        anyhow::bail!("unsupported DIB header size: {dib_size}");
    }
    let width = i32::from_le_bytes(b[18..22].try_into().unwrap());
    let height = i32::from_le_bytes(b[22..26].try_into().unwrap());
    let planes = u16::from_le_bytes(b[26..28].try_into().unwrap());
    let bpp = u16::from_le_bytes(b[28..30].try_into().unwrap());
    let comp = u32::from_le_bytes(b[30..34].try_into().unwrap());
    if planes != 1 {
        anyhow::bail!("unsupported planes={planes}");
    }
    if width <= 0 || height == 0 {
        anyhow::bail!("unsupported dims {width}x{height}");
    }
    let abs_h = height.abs();

    let (r_mask, g_mask, b_mask, a_mask) = if comp == 3 || comp == 6 {
        // BITFIELDS (or ALPHABITFIELDS)
        let base = 14 + dib_size as usize;
        let rm = u32::from_le_bytes(b[base..base + 4].try_into().unwrap());
        let gm = u32::from_le_bytes(b[base + 4..base + 8].try_into().unwrap());
        let bm = u32::from_le_bytes(b[base + 8..base + 12].try_into().unwrap());
        let am = if comp == 6 {
            u32::from_le_bytes(b[base + 12..base + 16].try_into().unwrap())
        } else {
            0
        };
        (rm, gm, bm, am)
    } else if bpp == 32 {
        (0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)
    } else {
        (0, 0, 0, 0)
    };

    let row_bytes = (((bpp as usize * width as usize) + 31) / 32) * 4;
    if pixel_off + row_bytes * (abs_h as usize) > b.len() {
        anyhow::bail!("bmp pixel data truncated");
    }

    let mut out = vec![0u8; (width as usize) * (abs_h as usize) * 4];
    for y in 0..abs_h {
        let src_y = if height > 0 { abs_h - 1 - y } else { y };
        let src_row = pixel_off + (src_y as usize) * row_bytes;
        for x in 0..width {
            let di = ((y as usize) * (width as usize) + (x as usize)) * 4;
            match bpp {
                32 => {
                    let si = src_row + (x as usize) * 4;
                    let px = u32::from_le_bytes(b[si..si + 4].try_into().unwrap());
                    let (r, g, b2, a) = if comp == 3 || comp == 6 {
                        (
                            scale_mask(px, r_mask),
                            scale_mask(px, g_mask),
                            scale_mask(px, b_mask),
                            if a_mask != 0 {
                                scale_mask(px, a_mask)
                            } else {
                                255
                            },
                        )
                    } else {
                        // default BGRA
                        let bb = b[si + 0];
                        let gg = b[si + 1];
                        let rr = b[si + 2];
                        let aa = b[si + 3];
                        (rr, gg, bb, aa)
                    };
                    out[di + 0] = r;
                    out[di + 1] = g;
                    out[di + 2] = b2;
                    out[di + 3] = a;
                }
                24 => {
                    let si = src_row + (x as usize) * 3;
                    out[di + 0] = b[si + 2];
                    out[di + 1] = b[si + 1];
                    out[di + 2] = b[si + 0];
                    out[di + 3] = 255;
                }
                _ => anyhow::bail!("unsupported bpp={bpp}"),
            }
        }
    }

    Ok((width, abs_h, out))
}

pub fn write_bmp_rgba(path: &Path, width: i32, height: i32, rgba: &[u8]) -> anyhow::Result<()> {
    if width <= 0 || height <= 0 {
        anyhow::bail!("bad dims");
    }
    let w = width as usize;
    let h = height as usize;
    if rgba.len() != w * h * 4 {
        anyhow::bail!("bad rgba length");
    }
    let row_bytes = w * 4;
    let file_size = 54 + row_bytes * h;
    let mut b = Vec::with_capacity(file_size);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(file_size as u32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(width as i32).to_le_bytes());
    b.extend_from_slice(&(height as i32).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&((row_bytes * h) as u32).to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());

    // bottom-up BGRA
    for y in (0..h).rev() {
        for x in 0..w {
            let si = (y * w + x) * 4;
            let r = rgba[si + 0];
            let g = rgba[si + 1];
            let bb = rgba[si + 2];
            let a = rgba[si + 3];
            b.push(bb);
            b.push(g);
            b.push(r);
            b.push(a);
        }
    }
    fs::write(path, b)?;
    Ok(())
}

pub fn resize_rgba_bilinear(sw: i32, sh: i32, src: &[u8], tw: i32, th: i32) -> Vec<u8> {
    let sw = sw.max(1) as usize;
    let sh = sh.max(1) as usize;
    let tw = tw.max(1) as usize;
    let th = th.max(1) as usize;
    let mut out = vec![0u8; tw * th * 4];
    let sx = sw as f32 / tw as f32;
    let sy = sh as f32 / th as f32;
    for y in 0..th {
        let fy = (y as f32 + 0.5) * sy - 0.5;
        let y0 = fy.floor().clamp(0.0, (sh - 1) as f32) as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = fy - y0 as f32;
        for x in 0..tw {
            let fx = (x as f32 + 0.5) * sx - 0.5;
            let x0 = fx.floor().clamp(0.0, (sw - 1) as f32) as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = fx - x0 as f32;
            let w00 = (1.0 - wx) * (1.0 - wy);
            let w10 = wx * (1.0 - wy);
            let w01 = (1.0 - wx) * wy;
            let w11 = wx * wy;
            let i00 = (y0 * sw + x0) * 4;
            let i10 = (y0 * sw + x1) * 4;
            let i01 = (y1 * sw + x0) * 4;
            let i11 = (y1 * sw + x1) * 4;
            let di = (y * tw + x) * 4;
            for c in 0..4 {
                let v = (src[i00 + c] as f32) * w00
                    + (src[i10 + c] as f32) * w10
                    + (src[i01 + c] as f32) * w01
                    + (src[i11 + c] as f32) * w11;
                out[di + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

fn tz(v: u32) -> u32 {
    if v == 0 {
        0
    } else {
        v.trailing_zeros()
    }
}

fn mask_bits(mask: u32) -> (u32, u32) {
    if mask == 0 {
        return (0, 0);
    }
    let shift = tz(mask);
    let bits = ((mask >> shift).count_ones()).max(0);
    (shift, bits)
}

fn scale_to_u8(value: u32, bits: u32) -> u8 {
    if bits == 0 {
        return 0;
    }
    if bits >= 8 {
        return (value & 0xFF) as u8;
    }
    let maxv = (1u32 << bits) - 1;
    ((value * 255 + maxv / 2) / maxv).min(255) as u8
}

fn scale_mask(px: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let (shift, bits) = mask_bits(mask);
    let v = (px & mask) >> shift;
    scale_to_u8(v, bits)
}
