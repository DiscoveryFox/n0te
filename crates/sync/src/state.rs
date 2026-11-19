//! What the engine remembers, in `<root>/sync/*.json` (and the key in `<root>/device.key`):
//! paired devices, invites we gave out, invites we joined, pending pair secrets.

use std::fs;
use std::path::{Path, PathBuf};

use iroh::{EndpointAddr, SecretKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::Result;
use crate::proto::Role;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub addr: EndpointAddr,
    pub paired: u64,
    #[serde(default)]
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InviteRecord {
    pub id: String,
    pub notebook: String,
    pub title: String,
    pub role: Role,
    pub expires: u64,
    pub created: u64,
    pub revoked: bool,
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinedRecord {
    pub link: String,
    pub invite: String,
    pub notebook: String,
    pub title: String,
    pub role: Role,
    pub owner: String,
    pub owner_name: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingPair {
    pub secret: [u8; 32],
    pub expires: u64,
}

#[derive(Debug, Default)]
pub struct State {
    pub devices: Vec<DeviceRecord>,
    pub invites: Vec<InviteRecord>,
    pub joined: Vec<JoinedRecord>,
    pub pending: Vec<PendingPair>,
    dir: PathBuf,
}

fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn store<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(v).expect("serializable");
    n0te_core::store::write_atomic(path, &bytes)?;
    Ok(())
}

impl State {
    pub fn load(root: &Path) -> Result<Self> {
        let dir = root.join("sync");
        fs::create_dir_all(&dir)?;
        Ok(State {
            devices: load(&dir.join("devices.json")),
            invites: load(&dir.join("invites.json")),
            joined: load(&dir.join("joined.json")),
            pending: load(&dir.join("pending-pairs.json")),
            dir,
        })
    }

    /// Pending pair secrets may be added by another process (the CLI): re-read them.
    pub fn reload_pending(&mut self) {
        self.pending = load(&self.dir.join("pending-pairs.json"));
    }

    pub fn save_devices(&self) -> Result<()> {
        store(&self.dir.join("devices.json"), &self.devices)
    }
    pub fn save_invites(&self) -> Result<()> {
        store(&self.dir.join("invites.json"), &self.invites)
    }
    pub fn save_joined(&self) -> Result<()> {
        store(&self.dir.join("joined.json"), &self.joined)
    }
    pub fn save_pending(&self) -> Result<()> {
        store(&self.dir.join("pending-pairs.json"), &self.pending)
    }
}

/// This library's iroh identity (`<root>/device.key`, 32 bytes, owner-only), created once.
pub fn load_or_create_key(root: &Path) -> Result<SecretKey> {
    let path = root.join("device.key");
    if let Ok(b) = fs::read(&path)
        && let Ok(bytes) = <[u8; 32]>::try_from(b.as_slice())
    {
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let key = SecretKey::generate();
    n0te_core::store::write_atomic(&path, &key.to_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}
