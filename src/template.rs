use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Key {
    pub species: u16,
    pub form: u16,
    pub gender: u8,
}

impl From<crate::fb::trpmcatalog::SpeciesKey> for Key {
    fn from(k: crate::fb::trpmcatalog::SpeciesKey) -> Self {
        Self {
            species: k.species,
            form: k.form,
            gender: k.gender,
        }
    }
}

impl From<Key> for crate::fb::trpmcatalog::SpeciesKey {
    fn from(k: Key) -> Self {
        Self {
            species: k.species,
            form: k.form,
            gender: k.gender,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub target: Key,
    pub donor: Key,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DonorTemplate {
    pub version: u32,
    pub language: String,

    pub include_targets_already_in_za: bool,

    pub default_donor: Option<Key>,
    pub donor_palette: Vec<Key>,

    pub selected_targets: Vec<Key>,
    pub assignments: Vec<Assignment>,
}

impl Default for DonorTemplate {
    fn default() -> Self {
        Self {
            version: 1,
            language: "English".to_string(),
            include_targets_already_in_za: false,
            default_donor: None,
            donor_palette: Vec::new(),
            selected_targets: Vec::new(),
            assignments: Vec::new(),
        }
    }
}

impl DonorTemplate {
    pub fn selected_set(&self) -> BTreeSet<Key> {
        self.selected_targets.iter().copied().collect()
    }

    pub fn assignment_map(&self) -> BTreeMap<Key, Key> {
        let mut out = BTreeMap::new();
        for a in &self.assignments {
            out.insert(a.target, a.donor);
        }
        out
    }

    pub fn set_assignment(&mut self, target: Key, donor: Key) {
        if let Some(a) = self.assignments.iter_mut().find(|a| a.target == target) {
            a.donor = donor;
            return;
        }
        self.assignments.push(Assignment { target, donor });
    }
}

pub struct TemplateStore {
    pub dir: PathBuf,
}

impl TemplateStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn ensure_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.dir)?;
        Ok(())
    }

    pub fn autosave_path(&self) -> PathBuf {
        self.dir.join("autosave.json")
    }

    pub fn load_or_default(&self, path: Option<&Path>) -> DonorTemplate {
        let p = path
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.autosave_path());
        let Ok(text) = fs::read_to_string(&p) else {
            return DonorTemplate::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self, tpl: &DonorTemplate, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(tpl)? + "\n")?;
        Ok(())
    }
}

pub fn preferred_template_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join("templates"));
        }
    }

    if let Some(proj) = directories::ProjectDirs::from("dev", "gftool", "svza") {
        out.push(proj.config_dir().join("templates"));
    }

    out
}
