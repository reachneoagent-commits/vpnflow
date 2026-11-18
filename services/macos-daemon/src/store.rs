//! Config store: one `<uuid>.conf` (mode 0600) per profile in a root-only
//! directory. Same trust model as NetworkManager's system connections on
//! Linux: root can read keys, other users cannot. The directory is checked
//! before every use, so a pre-created or loosened one is refused.

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use aegis_core::{CoreError, ProfileId, Result};
use uuid::Uuid;

pub const STATE_DIR: &str = "/var/db/vpnflow";

pub struct ConfigStore {
    dir: PathBuf,
    /// Required owner of the directory (0 in production; tests use their own uid).
    owner: u32,
}

impl ConfigStore {
    pub fn production() -> Result<Self> {
        Self::open(Path::new(STATE_DIR), 0)
    }

    pub fn open(dir: &Path, owner: u32) -> Result<Self> {
        if !dir.exists() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(CoreError::storage)?;
        }
        let s = Self {
            dir: dir.to_owned(),
            owner,
        };
        s.check_dir()?;
        Ok(s)
    }

    fn check_dir(&self) -> Result<()> {
        let md = std::fs::symlink_metadata(&self.dir).map_err(CoreError::storage)?;
        if !md.is_dir() || md.uid() != self.owner || md.mode() & 0o077 != 0 {
            return Err(CoreError::storage(
                "the VPNFlow state directory is not private to root; remove it and restart the daemon",
            ));
        }
        Ok(())
    }

    fn path(&self, id: ProfileId) -> PathBuf {
        self.dir.join(format!("{id}.conf"))
    }

    pub fn save(&self, id: ProfileId, text: &str) -> Result<()> {
        self.check_dir()?;
        let tmp = self.dir.join(format!(".{id}.tmp"));
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(CoreError::storage)?;
        f.write_all(text.as_bytes())
            .and_then(|()| f.sync_all())
            .map_err(CoreError::storage)?;
        std::fs::rename(&tmp, self.path(id)).map_err(CoreError::storage)
    }

    pub fn load(&self, id: ProfileId) -> Result<zeroize::Zeroizing<String>> {
        self.check_dir()?;
        let p = self.path(id);
        let md = std::fs::symlink_metadata(&p).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CoreError::NotFound,
            _ => CoreError::storage(e),
        })?;
        if !md.is_file() || md.mode() & 0o077 != 0 {
            return Err(CoreError::storage(
                "a stored profile has unsafe permissions",
            ));
        }
        std::fs::read_to_string(p)
            .map(zeroize::Zeroizing::new)
            .map_err(CoreError::storage)
    }

    pub fn remove(&self, id: ProfileId) -> Result<()> {
        self.check_dir()?;
        match std::fs::remove_file(self.path(id)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(CoreError::storage(e)),
        }
    }

    pub fn list(&self) -> Result<Vec<ProfileId>> {
        self.check_dir()?;
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(CoreError::storage)? {
            let name = entry.map_err(CoreError::storage)?.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".conf"))
                .and_then(|n| Uuid::parse_str(n).ok())
            {
                ids.push(ProfileId(id));
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmp() -> PathBuf {
        std::env::temp_dir().join(format!("vpnflow-store-{}", Uuid::new_v4()))
    }

    #[test]
    fn save_load_list_remove() {
        let dir = tmp();
        let me = {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::metadata(&dir).unwrap().uid()
        };
        let s = ConfigStore::open(&dir, me).unwrap();
        let id = ProfileId::new();
        s.save(id, "secret").unwrap();
        assert_eq!(&*s.load(id).unwrap(), "secret");
        assert_eq!(s.list().unwrap(), vec![id]);
        let mode = std::fs::metadata(dir.join(format!("{id}.conf")))
            .unwrap()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        s.remove(id).unwrap();
        s.remove(id).unwrap();
        assert!(matches!(s.load(id), Err(CoreError::NotFound)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_a_loose_directory() {
        let dir = tmp();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let me = std::fs::metadata(&dir).unwrap().uid();
        assert!(ConfigStore::open(&dir, me).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
