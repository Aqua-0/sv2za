use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn flatc_dump_json(
    flatc: &Path,
    schema: &Path,
    includes: &[PathBuf],
    src_bin: &Path,
    out_dir: &Path,
) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(out_dir)?;
    let mut cmd = Command::new(flatc);
    for inc in includes {
        cmd.arg("-I").arg(inc);
    }
    cmd.arg("--raw-binary")
        .arg("--strict-json")
        .arg("-t")
        .arg("-o")
        .arg(out_dir)
        .arg(schema)
        .arg("--")
        .arg(src_bin);
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!(
            "flatc dump failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    let expected = out_dir.join(format!(
        "{}.json",
        src_bin
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    ));
    if expected.is_file() {
        return Ok(expected);
    }
    let mut cands = Vec::new();
    for e in fs::read_dir(out_dir)? {
        let e = e?;
        if e.file_type()?.is_file() && e.path().extension().and_then(|x| x.to_str()) == Some("json")
        {
            cands.push(e.path());
        }
    }
    cands.sort();
    if cands.len() == 1 {
        return Ok(cands[0].clone());
    }
    anyhow::bail!("flatc did not write expected json under {out_dir:?}");
}

pub fn flatc_build_bin(
    flatc: &Path,
    schema: &Path,
    includes: &[PathBuf],
    src_json: &Path,
    out_bin: &Path,
) -> anyhow::Result<()> {
    if let Some(parent) = out_bin.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = tempfile::tempdir()?;
    let mut cmd = Command::new(flatc);
    for inc in includes {
        cmd.arg("-I").arg(inc);
    }
    cmd.arg("--raw-binary")
        .arg("--strict-json")
        .arg("-b")
        .arg("-o")
        .arg(tmp.path())
        .arg(schema)
        .arg(src_json);
    let out = cmd.output()?;
    if !out.status.success() {
        anyhow::bail!(
            "flatc build failed: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout)
        );
    }
    let mut outs = Vec::new();
    for e in fs::read_dir(tmp.path())? {
        let e = e?;
        if e.file_type()?.is_file() {
            outs.push(e.path());
        }
    }
    outs.sort();
    if outs.len() != 1 {
        anyhow::bail!("flatc wrote unexpected outputs: {outs:?}");
    }
    fs::copy(&outs[0], out_bin)?;
    Ok(())
}
