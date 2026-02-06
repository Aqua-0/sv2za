use clap::Parser;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub sv_root: Option<PathBuf>,
    pub za_dump: Option<PathBuf>,
    pub out_root: Option<PathBuf>,
    pub ultimate_tex_cli: Option<PathBuf>,
    pub flatc: Option<PathBuf>,
    pub pknx_personal_dir: Option<PathBuf>,

    pub language: String,

    pub texture_convert: bool,
    pub texture_allow_resize: bool,
    pub use_za_base_config: bool,
    pub za_base_donor_pm_variant: String,
    pub no_head_look_at: bool,

    /// When enabled, do not process mons whose (species,form,gender) key already exists in ZA's catalog
    /// When disabled, process them anyway (useful for ReZAifying an existing mon to debug animation/config issues)
    pub skip_pokemon_already_in_za: bool,

    /// Show legacy toggles/settings UI. New workflow uses templates + donor assignments instead
    pub legacy_mode: bool,

    /// When enabled, write debugging reports under `Output/_report`
    pub generate_reports: bool,

    pub donor_dev: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            sv_root: None,
            za_dump: None,
            out_root: None,
            ultimate_tex_cli: None,
            flatc: None,
            pknx_personal_dir: None,
            language: "English".to_string(),
            texture_convert: false,
            texture_allow_resize: true,
            use_za_base_config: false,
            za_base_donor_pm_variant: "pm0866_00_00".to_string(),
            no_head_look_at: false,
            skip_pokemon_already_in_za: true,
            legacy_mode: false,
            generate_reports: true,
            donor_dev: 866,
        }
    }
}

impl AppConfig {
    pub fn load_or_default() -> anyhow::Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        let cfg = serde_json::from_str::<Self>(&text)?;
        Ok(cfg)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)? + "\n";
        fs::write(path, text)?;
        Ok(())
    }

    pub fn apply_headless(&mut self, args: &HeadlessArgs) {
        if let Some(p) = &args.sv_root {
            self.sv_root = Some(p.clone());
        }
        if let Some(p) = &args.za_dump {
            self.za_dump = Some(p.clone());
        }
        if let Some(p) = &args.out_root {
            self.out_root = Some(p.clone());
        }
        if let Some(p) = &args.ultimate_tex_cli {
            self.ultimate_tex_cli = Some(p.clone());
        }
        if let Some(p) = &args.flatc {
            self.flatc = Some(p.clone());
        }
        if let Some(p) = &args.pknx_personal_dir {
            self.pknx_personal_dir = Some(p.clone());
        }
        if args.texture_convert {
            self.texture_convert = true;
        }
        if args.no_texture_resize {
            self.texture_allow_resize = false;
        }
        if args.use_za_base_config {
            self.use_za_base_config = true;
        }
        if let Some(s) = &args.za_base_donor_pm_variant {
            self.za_base_donor_pm_variant = s.clone();
        }
        if args.no_head_look_at {
            self.no_head_look_at = true;
        }
        // This is an explicit toggle (defaults true); apply unconditionally so passing `--skip-pokemon-already-in-za false`
        // works as expected
        self.skip_pokemon_already_in_za = args.skip_pokemon_already_in_za;
        self.legacy_mode = args.legacy_mode;
        self.generate_reports = args.generate_reports;
        if let Some(v) = args.donor_dev {
            self.donor_dev = v;
        }
        if let Some(s) = &args.lang {
            if !s.trim().is_empty() {
                self.language = s.trim().to_string();
            }
        }
    }
}

fn config_path() -> anyhow::Result<PathBuf> {
    let proj = ProjectDirs::from("dev", "gftool", "svza")
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?;
    Ok(proj.config_dir().join("config.json"))
}

#[derive(Debug, Parser, Clone)]
#[command(author, version, about)]
pub struct HeadlessArgs {
    #[arg(long, default_value_t = false)]
    pub headless: bool,

    #[arg(long)]
    pub sv_root: Option<PathBuf>,

    #[arg(long)]
    pub za_dump: Option<PathBuf>,

    #[arg(long)]
    pub out_root: Option<PathBuf>,

    #[arg(long)]
    pub ultimate_tex_cli: Option<PathBuf>,

    #[arg(long)]
    pub flatc: Option<PathBuf>,

    #[arg(long)]
    pub pknx_personal_dir: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub texture_convert: bool,

    #[arg(long, default_value_t = false)]
    pub no_texture_resize: bool,

    #[arg(long, default_value_t = false)]
    pub use_za_base_config: bool,

    #[arg(long)]
    pub za_base_donor_pm_variant: Option<String>,

    #[arg(long, default_value_t = false)]
    pub no_head_look_at: bool,

    /// If true (default), skip mons already present in ZA's catalog
    /// Pass `--skip-pokemon-already-in-za false` to process them anyway
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub skip_pokemon_already_in_za: bool,

    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub legacy_mode: bool,

    /// If true (default), write debugging reports under `Output/_report`
    /// Pass `--generate-reports false` to disable
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub generate_reports: bool,

    #[arg(long)]
    pub donor_dev: Option<u32>,

    #[arg(long)]
    pub lang: Option<String>,
}
