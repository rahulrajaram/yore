use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::types::*;

pub fn load_config(path: &Path, quiet: bool) -> Option<YoreConfig> {
    if !path.exists() {
        return None;
    }

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            if !quiet {
                eprintln!(
                    "{}: failed to read config {}: {}",
                    "warning".yellow(),
                    path.display(),
                    e
                );
            }
            return None;
        }
    };

    match toml::from_str::<YoreConfig>(&contents) {
        Ok(cfg) => Some(cfg),
        Err(e) => {
            if !quiet {
                eprintln!(
                    "{}: failed to parse config {}: {}",
                    "warning".yellow(),
                    path.display(),
                    e
                );
            }
            None
        }
    }
}

pub fn resolve_build_params(
    path: PathBuf,
    output: PathBuf,
    types: String,
    profile: Option<&str>,
    config: &Option<YoreConfig>,
) -> (PathBuf, PathBuf, String, Option<Vec<PathBuf>>) {
    // Defaults from CLI definition
    let default_path = PathBuf::from(".");
    let default_output = PathBuf::from(".yore");
    let default_types = "md,txt,rst".to_string();

    let mut effective_path = path;
    let mut effective_output = output;
    let mut effective_types = types;
    let mut roots: Option<Vec<PathBuf>> = None;

    if let (Some(profile_name), Some(cfg)) = (profile, config.as_ref()) {
        if let Some(profile_cfg) = cfg.index.get(profile_name) {
            // Roots: if present, use them as allowed roots (multi-root support)
            if !profile_cfg.roots.is_empty() {
                let rs: Vec<PathBuf> = profile_cfg.roots.iter().map(PathBuf::from).collect();
                roots = Some(rs);
                // Use repo root (".") as walk root when using multiple roots
                effective_path.clone_from(&default_path);
            }

            // Types: only override when CLI used the default
            if effective_types == default_types && !profile_cfg.types.is_empty() {
                effective_types = profile_cfg.types.join(",");
            }

            // Output: only override when CLI used the default
            if effective_output == default_output {
                if let Some(ref out) = profile_cfg.output {
                    effective_output = PathBuf::from(out);
                }
            }
        }
    }

    (effective_path, effective_output, effective_types, roots)
}

pub fn resolve_index_path(
    index: PathBuf,
    profile: Option<&str>,
    config: &Option<YoreConfig>,
) -> PathBuf {
    let default_index = PathBuf::from(".yore");

    if index != default_index {
        return index;
    }

    if let (Some(profile_name), Some(cfg)) = (profile, config.as_ref()) {
        if let Some(profile_cfg) = cfg.index.get(profile_name) {
            if let Some(ref out) = profile_cfg.output {
                return PathBuf::from(out);
            }
        }
    }

    index
}
