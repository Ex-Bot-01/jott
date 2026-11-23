//! Is this folder synced by a cloud service?
//!
//! Such services copy a folder file by file. A `.git` inside one gets its objects and refs copied out of order
//! between machines, which is how repositories end up corrupt — so for these folders the repository lives in Jott's
//! app data instead (spec: *Cloud-synced folders*). Detection reads each service's own markers and settings; it is a
//! pure function over [`CloudProbe`], so every service is tested without any of them installed.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudService {
    OneDrive,
    Dropbox,
    GoogleDrive,
    ICloud,
    Box,
    Nextcloud,
    Syncthing,
    /// A provider in macOS's `~/Library/CloudStorage` that is not one of the above.
    Other,
}

impl CloudService {
    /// The name people know it by, for sentences.
    pub fn name(self) -> &'static str {
        match self {
            CloudService::OneDrive => "OneDrive",
            CloudService::Dropbox => "Dropbox",
            CloudService::GoogleDrive => "Google Drive",
            CloudService::ICloud => "iCloud Drive",
            CloudService::Box => "Box",
            CloudService::Nextcloud => "Nextcloud",
            CloudService::Syncthing => "Syncthing",
            CloudService::Other => "A cloud storage service",
        }
    }
}

pub trait CloudProbe {
    fn env(&self, key: &str) -> Option<PathBuf>;
    fn home(&self) -> Option<PathBuf>;
    fn read(&self, path: &Path) -> Option<String>;
    /// File and folder names directly inside `dir`; empty when it cannot be read.
    fn names_in(&self, dir: &Path) -> Vec<String>;
    /// Windows and macOS compare paths without regard to case.
    fn case_insensitive(&self) -> bool;
}

pub struct SystemCloudProbe;

impl CloudProbe for SystemCloudProbe {
    fn env(&self, key: &str) -> Option<PathBuf> {
        std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    fn home(&self) -> Option<PathBuf> {
        self.env(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
    }

    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn names_in(&self, dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }

    fn case_insensitive(&self) -> bool {
        cfg!(any(windows, target_os = "macos"))
    }
}

/// The service syncing `vault`, if any. `vault` should be absolute.
pub fn cloud_service(vault: &Path, probe: &dyn CloudProbe) -> Option<CloudService> {
    let fold = probe.case_insensitive();
    let inside = |root: &Path| starts_with(vault, root, fold);

    for key in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if probe.env(key).is_some_and(|root| inside(&root)) {
            return Some(CloudService::OneDrive);
        }
    }

    for info in dropbox_info_files(probe) {
        if probe.read(&info).is_some_and(|json| dropbox_roots(&json).iter().any(|root| inside(root))) {
            return Some(CloudService::Dropbox);
        }
    }

    if let Some(home) = probe.home() {
        let storage = home.join("Library").join("CloudStorage");
        if inside(&storage) {
            let provider = vault.strip_prefix(&storage).ok().and_then(|rest| rest.components().next());
            let provider = provider.map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
            return Some(match provider {
                p if p.starts_with("onedrive") => CloudService::OneDrive,
                p if p.starts_with("dropbox") => CloudService::Dropbox,
                p if p.starts_with("googledrive") => CloudService::GoogleDrive,
                p if p.starts_with("box") => CloudService::Box,
                p if p.starts_with("icloud") => CloudService::ICloud,
                _ => CloudService::Other,
            });
        }
        let named = [
            (vec!["Library", "Mobile Documents"], CloudService::ICloud),
            (vec!["iCloudDrive"], CloudService::ICloud),
            (vec!["OneDrive"], CloudService::OneDrive),
            (vec!["Dropbox"], CloudService::Dropbox),
            (vec!["Google Drive"], CloudService::GoogleDrive),
            (vec!["Box"], CloudService::Box),
        ];
        for (parts, service) in named {
            let root = parts.iter().fold(home.clone(), |path, part| path.join(part));
            if inside(&root) {
                return Some(service);
            }
        }
    }

    // Google Drive for desktop names its folder "My Drive", wherever it mounts it.
    if vault.components().any(|c| matches!(c, Component::Normal(name) if name == "My Drive")) {
        return Some(CloudService::GoogleDrive);
    }

    // Markers the services keep at the root of what they sync.
    let mut dir = Some(vault);
    while let Some(here) = dir {
        let names = probe.names_in(here);
        if names.iter().any(|n| n == ".stfolder") {
            return Some(CloudService::Syncthing);
        }
        if names.iter().any(|n| n == ".dropbox" || n == ".dropbox.cache") {
            return Some(CloudService::Dropbox);
        }
        if names.iter().any(|n| is_owncloud_journal(n)) {
            return Some(CloudService::Nextcloud);
        }
        dir = here.parent();
    }
    None
}

fn dropbox_info_files(probe: &dyn CloudProbe) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(root) = probe.env(key) {
            out.push(root.join("Dropbox").join("info.json"));
        }
    }
    if let Some(home) = probe.home() {
        out.push(home.join(".dropbox").join("info.json"));
    }
    out
}

/// Every `"path"` in Dropbox's `info.json` (`{"personal": {"path": …}, "business": {"path": …}}`).
fn dropbox_roots(json: &str) -> Vec<PathBuf> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let Some(accounts) = value.as_object() else { return Vec::new() };
    accounts
        .values()
        .filter_map(|account| account.get("path").and_then(|p| p.as_str()))
        .map(PathBuf::from)
        .collect()
}

/// Nextcloud and ownCloud keep a sync journal at the root they sync: `.sync_<hex>.db`, `._sync_<hex>.db`,
/// `.sync_journal.db`, plus their `-wal`/`-shm` companions.
fn is_owncloud_journal(name: &str) -> bool {
    let rest = name.strip_prefix("._sync_").or_else(|| name.strip_prefix(".sync_"));
    rest.is_some_and(|rest| rest.ends_with(".db") || rest.ends_with(".db-wal") || rest.ends_with(".db-shm"))
}

fn starts_with(path: &Path, root: &Path, fold: bool) -> bool {
    let mut path = path.components();
    for want in root.components() {
        let Some(have) = path.next() else { return false };
        let (have, want) = (have.as_os_str().to_string_lossy(), want.as_os_str().to_string_lossy());
        let same = if fold { have.to_lowercase() == want.to_lowercase() } else { have == want };
        if !same {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        env: HashMap<&'static str, PathBuf>,
        home: Option<PathBuf>,
        files: HashMap<PathBuf, String>,
        dirs: HashMap<PathBuf, Vec<String>>,
        fold: bool,
    }

    impl CloudProbe for Fake {
        fn env(&self, key: &str) -> Option<PathBuf> {
            self.env.get(key).cloned()
        }
        fn home(&self) -> Option<PathBuf> {
            self.home.clone()
        }
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn names_in(&self, dir: &Path) -> Vec<String> {
            self.dirs.get(dir).cloned().unwrap_or_default()
        }
        fn case_insensitive(&self) -> bool {
            self.fold
        }
    }

    fn home() -> Fake {
        Fake { home: Some(PathBuf::from("/home/ana")), ..Fake::default() }
    }

    #[test]
    fn a_plain_folder_is_not_synced() {
        assert_eq!(cloud_service(Path::new("/home/ana/notes"), &home()), None);
    }

    #[test]
    fn onedrive_is_known_from_its_environment_and_case_does_not_matter_where_it_does_not() {
        let mut probe = home();
        probe.env.insert("OneDriveCommercial", PathBuf::from("/home/ana/OneDrive - Siemens"));
        assert_eq!(cloud_service(Path::new("/home/ana/OneDrive - Siemens/notes"), &probe), Some(CloudService::OneDrive));
        assert_eq!(cloud_service(Path::new("/home/ana/onedrive - siemens/notes"), &probe), None);
        probe.fold = true;
        assert_eq!(cloud_service(Path::new("/home/ana/onedrive - siemens/notes"), &probe), Some(CloudService::OneDrive));
    }

    #[test]
    fn a_folder_that_merely_shares_a_prefix_is_not_inside() {
        let mut probe = home();
        probe.env.insert("OneDrive", PathBuf::from("/home/ana/OneDrive"));
        probe.home = None;
        assert_eq!(cloud_service(Path::new("/home/ana/OneDriveBackup/notes"), &probe), None);
    }

    #[test]
    fn dropbox_is_known_from_its_info_file_wherever_it_put_the_folder() {
        let mut probe = home();
        probe.files.insert(
            PathBuf::from("/home/ana/.dropbox/info.json"),
            r#"{"personal": {"path": "/data/Dropbox", "host": 1}}"#.into(),
        );
        assert_eq!(cloud_service(Path::new("/data/Dropbox/notes"), &probe), Some(CloudService::Dropbox));
    }

    #[test]
    fn markers_at_a_synced_root_are_found_from_a_folder_below_it() {
        for (marker, service) in [
            (".stfolder", CloudService::Syncthing),
            (".dropbox", CloudService::Dropbox),
            (".sync_4f2a9c.db", CloudService::Nextcloud),
            ("._sync_4f2a9c.db", CloudService::Nextcloud),
        ] {
            let mut probe = home();
            probe.dirs.insert(PathBuf::from("/srv/share"), vec![marker.to_string(), "notes".into()]);
            assert_eq!(cloud_service(Path::new("/srv/share/notes/work"), &probe), Some(service), "{marker}");
        }
    }

    #[test]
    fn the_platform_s_own_cloud_folders_are_known() {
        let probe = home();
        for (vault, service) in [
            ("/home/ana/Library/Mobile Documents/com~apple~CloudDocs/notes", CloudService::ICloud),
            ("/home/ana/Library/CloudStorage/GoogleDrive-ana@example.com/notes", CloudService::GoogleDrive),
            ("/home/ana/Library/CloudStorage/OneDrive-Personal/notes", CloudService::OneDrive),
            ("/home/ana/Library/CloudStorage/SomethingNew/notes", CloudService::Other),
            ("/home/ana/iCloudDrive/notes", CloudService::ICloud),
            ("/home/ana/Box/notes", CloudService::Box),
            ("/mnt/g/My Drive/notes", CloudService::GoogleDrive),
        ] {
            assert_eq!(cloud_service(Path::new(vault), &probe), Some(service), "{vault}");
        }
    }
}
