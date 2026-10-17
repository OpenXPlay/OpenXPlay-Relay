// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-16

//! Installed package discovery only; the relay never executes plugin code.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

pub fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id.contains('.')
        && id.split('.').enumerate().all(|(index, label)| {
            !label.is_empty()
                && label.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || (index != 0 && c == b'-')
                })
        })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub id: String,
    pub name: BTreeMap<String, String>,
    pub version: String,
    pub license: String,
    /// An image data URL, never HTML or a remote URL.
    pub icon: Option<String>,
    pub non_commercial_assets: bool,
}
fn read_inside(root: &Path, relative: &Path, limit: u64) -> Option<Vec<u8>> {
    let path = root.join(relative).canonicalize().ok()?;
    if !path.starts_with(root) {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}
fn manifest(root: &Path, id: &str) -> Option<serde_json::Value> {
    if !valid_id(id) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let bytes = read_inside(
        &root,
        &PathBuf::from("p").join(id).join("oxp-plugin.json"),
        65536,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    (value["id"] == id && value["spec"] == "oxp-web-plugin/0").then_some(value)
}
pub fn installed(root: &Path) -> Vec<InstalledPlugin> {
    let Ok(root) = root.canonicalize() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(root.join("p")) else {
        return Vec::new();
    };
    let mut plugins: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().into_string().ok()?;
            let value = manifest(&root, &id)?;
            let name: BTreeMap<String, String> =
                serde_json::from_value(value["name"].clone()).ok()?;
            if !name.contains_key("en") {
                return None;
            }
            let license = value["license"].as_str()?.to_owned();
            let icon = value["icon"].as_str().and_then(|path| {
                let relative = Path::new(path);
                if relative
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
                {
                    return None;
                }
                let mime = match relative.extension()?.to_str()? {
                    "svg" => "image/svg+xml",
                    "png" => "image/png",
                    _ => return None,
                };
                let package = root.join("p").join(&id).canonicalize().ok()?;
                if !package.starts_with(&root) {
                    return None;
                }
                let bytes = read_inside(&package, relative, 65536)?;
                let encoded: String = bytes.iter().map(|b| format!("%{b:02X}")).collect();
                Some(format!("data:{mime},{encoded}"))
            });
            let non_commercial_assets = license
                .to_ascii_uppercase()
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|term| matches!(term, "NC" | "NONCOMMERCIAL"));
            Some(InstalledPlugin {
                id,
                name,
                version: value["version"].as_str()?.into(),
                license,
                icon,
                non_commercial_assets,
            })
        })
        .collect();
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}
#[derive(Clone, Debug)]
pub struct OutputPlugins {
    pub root: PathBuf,
    pub enabled: Vec<String>,
}
impl OutputPlugins {
    pub fn present_ids(&self) -> Vec<String> {
        self.enabled
            .iter()
            .filter(|id| manifest(&self.root, id).is_some())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn package(root: &Path, id: &str, license: &str) {
        let dir = root.join("p").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("icon.svg"),
            b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        )
        .unwrap();
        std::fs::write(
            dir.join("oxp-plugin.json"),
            serde_json::to_vec(&serde_json::json!({
                "spec":"oxp-web-plugin/0", "id":id, "name":{"en":"Pet", "zh-CN":"宠物"},
                "icon":"icon.svg", "version":"1.0.0", "license":license,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn catalog_and_paths_are_local_bounded_and_contained() {
        let root = std::env::temp_dir().join(format!(
            "plugin-catalog-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        package(
            &root,
            "org.openxplay.pet",
            "(Apache-2.0 OR MIT) AND CC-BY-NC-SA-4.0",
        );
        package(&root, "org.openxplay.clock", "MIT");
        let plugins = installed(&root);
        assert_eq!(
            plugins.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["org.openxplay.clock", "org.openxplay.pet"]
        );
        assert!(!plugins[0].non_commercial_assets);
        assert!(plugins[1].non_commercial_assets);
        assert!(plugins[1]
            .icon
            .as_ref()
            .unwrap()
            .starts_with("data:image/svg+xml,%3C"));
        let path = root.join("p/org.openxplay.pet/oxp-plugin.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        for icon in [
            "../../secret.png",
            "/tmp/icon.png",
            "https://example.com/icon.png",
        ] {
            value["icon"] = icon.into();
            std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(installed(&root)[1].icon.is_none());
        }
        let outside = root.with_extension("outside");
        package(&outside, "org.escape", "MIT");
        std::os::unix::fs::symlink(outside.join("p/org.escape"), root.join("p/org.escape"))
            .unwrap();
        assert_eq!(installed(&root).len(), 2);
        assert!(manifest(&root, "org.escape").is_none());
        std::fs::remove_dir_all(outside).unwrap();
        std::fs::write(&path, vec![b'x'; 65537]).unwrap();
        assert_eq!(installed(&root).len(), 1);
        for bad in [
            "",
            "org",
            "../org.plugin",
            "org.Plugin",
            "org..plugin",
            "org.plugin/",
            "org,plugin.id",
        ] {
            assert!(!valid_id(bad));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod option_tests {
    #[test]
    fn root_option_defaults_and_accepts_an_override_without_enabling_plugins() {
        let dir = std::env::temp_dir().join(format!(
            "plugin-options-{}",
            crate::admin::auth::random_hex(8).unwrap()
        ));
        let args = vec!["--state-dir".into(), dir.to_string_lossy().into_owned()];
        let defaults = crate::Options::parse(&args).unwrap();
        assert_eq!(
            defaults.plugins_root,
            std::path::Path::new("/opt/openxplay/share/plugins")
        );
        assert!(!defaults.settings.plugins);
        assert!(defaults.settings.plugins_enabled.is_empty());
        let mut args = args;
        args.extend([
            "--plugins-root".into(),
            dir.join("packages").to_string_lossy().into_owned(),
        ]);
        let configured = crate::Options::parse(&args).unwrap();
        assert_eq!(configured.plugins_root, dir.join("packages"));
        assert!(!configured.settings.plugins);
        assert!(!dir.exists());
    }
}
