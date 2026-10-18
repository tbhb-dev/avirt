//! The guest artifact caches under `cache/` hold the kernel, the init filesystem, and a root
//! filesystem for each image.
//!
//! avirthostd decides what to prepare and whether a cache entry is valid. `avirtvmd prepare` makes
//! the bytes. A temporary directory next to the final place receives each entry, and a rename
//! puts it there after its digest checks pass. A missing or damaged entry gets one rebuild.

#[cfg(test)]
mod tests;

use super::digest;
use super::images::Layer;
use super::temp::{PREFIX, Temp};
use crate::console;
use crate::state::private_dir;
use alloc::sync::Arc;
use anyhow::{Context as _, Result, bail, ensure};
use async_trait::async_trait;
use core::fmt;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, MutexGuard, PoisonError};
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task;

/// The capacity of a root filesystem image. The file is sparse.
pub const ROOTFS_CAPACITY: u64 = 1 << 33;

/// The pins of the Containerization runtime.
pub const PINNED: Pins = Pins {
    kernel_member: "opt/kata/share/kata-containers/vmlinux-6.18.35-197-debug",
    kernel_sha256: "fb2cfb79eb1ae19447a85d75682d7fa5cfec97e24beb2609a492b806e8072c8d",
    kernel_url: "https://github.com/kata-containers/kata-containers/releases/download/3.32.0/kata-static-3.32.0-arm64.tar.zst",
    tarball_sha256: "8736c054d9223974735394f822000823baef509e1c33405ec798240fa9b6e4b5",
    vminit_digest: "sha256:4c1836052eafcbc944c403fb23ded06562962d2402c9dcc651495a32300614df",
    vminit_reference: "ghcr.io/apple/containerization/vminit:0.47.0",
};

/// The artifacts avirthostd pins by digest.
#[derive(Clone, Copy, Debug)]
pub struct Pins {
    /// The kernel's path inside the tarball.
    pub kernel_member: &'static str,
    /// The sha256 of the extracted kernel.
    pub kernel_sha256: &'static str,
    /// Where the kernel tarball comes from.
    pub kernel_url: &'static str,
    /// The sha256 of the kernel tarball.
    pub tarball_sha256: &'static str,
    /// The digest of the init image, as `sha256:<hex>`.
    pub vminit_digest: &'static str,
    /// The init image's reference.
    pub vminit_reference: &'static str,
}

/// Makes guest artifacts. The production version runs `avirtvmd prepare`.
#[async_trait]
pub trait Preparer: Send + Sync {
    /// Extracts one member of an archive.
    ///
    /// # Errors
    ///
    /// Returns an error if extraction fails.
    async fn extract(&self, archive: &Path, member: &str, out: &Path) -> Result<()>;

    /// Downloads `url` to `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if the download fails.
    async fn fetch(&self, url: &str, out: &Path) -> Result<()>;

    /// Writes the init image's filesystem to `out` and returns the image's digest.
    ///
    /// # Errors
    ///
    /// Returns an error if pulling or unpacking the image fails.
    async fn initfs(&self, reference: &str, store: &Path, out: &Path) -> Result<String>;

    /// Unpacks `layers`, bottom first, into an ext4 file at `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if unpacking a layer fails.
    async fn rootfs(&self, out: &Path, capacity: u64, layers: &[Layer]) -> Result<()>;
}

/// Runs `avirtvmd prepare`.
#[derive(Clone, Debug)]
pub struct AvirtvmdPreparer {
    /// The signed avirtvmd binary.
    bin: PathBuf,
}

impl AvirtvmdPreparer {
    /// A preparer that runs the binary at `bin`.
    #[must_use]
    pub const fn new(bin: PathBuf) -> Self {
        Self { bin }
    }

    /// Runs one `prepare` step and returns what it printed.
    ///
    /// # Errors
    ///
    /// Returns an error if the binary can't run or the step fails.
    async fn run(&self, step: &str, args: &[OsString]) -> Result<String> {
        let output = Command::new(&self.bin)
            .arg("prepare")
            .arg(step)
            .args(args)
            // Only what it needs: no credentials from avirthostd's environment.
            .env_clear()
            .envs(["PATH", "HOME", "TMPDIR"].iter().filter_map(|name| env::var_os(name).map(|value| (*name, value))))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .with_context(|| format!("run {} prepare {step}", self.bin.display()))?;
        if !output.status.success() {
            let note = String::from_utf8_lossy(&output.stderr);
            bail!("avirtvmd prepare {step} failed ({}): {}", output.status, note.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[async_trait]
impl Preparer for AvirtvmdPreparer {
    async fn extract(&self, archive: &Path, member: &str, out: &Path) -> Result<()> {
        let args = ["--archive".into(), archive.into(), "--member".into(), member.into(), "--out".into(), out.into()];
        self.run("extract", &args).await.map(drop)
    }

    async fn fetch(&self, url: &str, out: &Path) -> Result<()> {
        self.run("fetch", &["--url".into(), url.into(), "--out".into(), out.into()]).await.map(drop)
    }

    async fn initfs(&self, reference: &str, store: &Path, out: &Path) -> Result<String> {
        let args = ["--reference".into(), reference.into(), "--store".into(), store.into(), "--out".into(), out.into()];
        let printed = self.run("initfs", &args).await?;
        let reply: serde_json::Value = serde_json::from_str(&printed)?;
        let digest = reply.get("digest").and_then(serde_json::Value::as_str).context("initfs printed no digest")?;
        Ok(digest.to_owned())
    }

    async fn rootfs(&self, out: &Path, capacity: u64, layers: &[Layer]) -> Result<()> {
        let mut args: Vec<OsString> =
            vec!["--out".into(), out.into(), "--capacity".into(), capacity.to_string().into()];
        for layer in layers {
            let mut argument = OsString::from(&layer.media_type);
            argument.push("=");
            argument.push(&layer.path);
            args.extend(["--layer".into(), argument]);
        }
        self.run("rootfs", &args).await.map(drop)
    }
}

/// The lock for each entry directory.
type Locks = Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>;

/// What to build into a cache entry.
enum Wanted<'layers> {
    /// The init filesystem.
    Initfs,
    /// The kernel.
    Kernel,
    /// A root filesystem of these layers.
    Rootfs(&'layers [Layer]),
}

/// The evidence that a cache entry is intact.
enum Proof {
    /// The file hashes to this digest.
    Digest(String),
    /// The file hashes to what its `.sha256` sidecar says.
    Sidecar,
}

/// A directory with one file in it, which makes one cache entry.
struct Entry {
    /// The directory, named by the entry's key.
    dir: PathBuf,
    /// The file's name.
    file: &'static str,
    /// The name in messages.
    label: &'static str,
    /// The evidence that the entry is intact.
    proof: Proof,
}

impl Entry {
    /// The entry's file.
    fn path(&self) -> PathBuf {
        self.dir.join(self.file)
    }
}

/// The caches under `cache/`, with the lock that keeps two launches from building one entry twice.
#[derive(Debug)]
pub struct Artifacts {
    /// One lock per entry directory.
    locks: Locks,
    /// The pins.
    pins: Pins,
    /// What makes the bytes.
    preparer: Arc<dyn Preparer>,
    /// The `cache/` directory.
    root: PathBuf,
    /// Sidecar entries that this process has already checked.
    verified: Mutex<HashSet<PathBuf>>,
}

impl fmt::Debug for dyn Preparer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Preparer")
    }
}

impl Artifacts {
    /// Builds the files of an entry in `temp`.
    ///
    /// # Errors
    ///
    /// Returns an error if a step fails or a digest doesn't match.
    async fn build(&self, wanted: &Wanted<'_>, temp: &Path) -> Result<()> {
        match wanted {
            Wanted::Initfs => self.build_initfs(temp).await,
            Wanted::Kernel => self.build_kernel(temp).await,
            Wanted::Rootfs(layers) => {
                let out = temp.join("rootfs.ext4");
                self.preparer.rootfs(&out, ROOTFS_CAPACITY, layers).await?;
                write_sidecar(&out).await
            }
        }
    }

    /// Builds the init filesystem and its sidecar.
    ///
    /// # Errors
    ///
    /// Returns an error if the image isn't the pinned one.
    async fn build_initfs(&self, temp: &Path) -> Result<()> {
        let (store, out) = (temp.join("store"), temp.join("initfs.ext4"));
        let printed = self.preparer.initfs(self.pins.vminit_reference, &store, &out).await?;
        ensure!(
            printed == self.pins.vminit_digest,
            "the init image has digest {printed}, not {}",
            self.pins.vminit_digest
        );
        fs::remove_dir_all(&store)?;
        write_sidecar(&out).await
    }

    /// Fetches the tarball and extracts the kernel, checking each against its pin.
    ///
    /// # Errors
    ///
    /// Returns an error if the tarball or the kernel doesn't hash to its pin.
    async fn build_kernel(&self, temp: &Path) -> Result<()> {
        let (tarball, kernel) = (temp.join("kernel.tar.zst"), temp.join("vmlinux"));
        self.preparer.fetch(self.pins.kernel_url, &tarball).await?;
        let found = hash_file(&tarball).await?;
        ensure!(
            found == self.pins.tarball_sha256,
            "the kernel tarball hashes to {found}, not {}",
            self.pins.tarball_sha256
        );
        self.preparer.extract(&tarball, self.pins.kernel_member, &kernel).await?;
        let kernel_hash = hash_file(&kernel).await?;
        ensure!(
            kernel_hash == self.pins.kernel_sha256,
            "the kernel hashes to {kernel_hash}, not {}",
            self.pins.kernel_sha256
        );
        fs::remove_file(&tarball)?;
        Ok(())
    }

    /// Builds the entry for `wanted`, or finds it intact.
    ///
    /// # Errors
    ///
    /// Returns an error if building the entry fails or a digest check fails.
    async fn ensure(&self, entry: &Entry, wanted: &Wanted<'_>) -> Result<PathBuf> {
        let lock = self.lock_for(&entry.dir);
        let _held = lock.lock().await;
        if self.is_intact(entry).await? {
            return Ok(entry.path());
        }
        let label = entry.label;
        if entry.dir.exists() {
            console::err_line(format_args!("{label} cache sha256 mismatch; fetching it again"));
            fs::remove_dir_all(&entry.dir)?;
        } else {
            console::err_line(format_args!("{label} cache missing; fetching it"));
        }
        let parent = entry.dir.parent().context("a cache entry has a parent")?;
        private_dir(parent)?;
        let temp = Temp::new(parent.join(format!("{PREFIX}{:016x}", rand::random::<u64>())));
        private_dir(temp.path())?;
        self.build(wanted, temp.path()).await.with_context(|| format!("build the {label} cache entry"))?;
        temp.persist(&entry.dir)?;
        self.verified.lock().unwrap_or_else(PoisonError::into_inner).insert(entry.dir.clone());
        Ok(entry.path())
    }

    /// The init filesystem, built if the cache lacks it or holds a damaged copy.
    ///
    /// # Errors
    ///
    /// Returns an error if building the entry fails or its digest differs from the pin.
    pub async fn initfs(&self) -> Result<PathBuf> {
        self.ensure(&self.initfs_entry(), &Wanted::Initfs).await
    }

    /// The entry for the init filesystem.
    fn initfs_entry(&self) -> Entry {
        let hex = self.pins.vminit_digest.strip_prefix("sha256:").unwrap_or(self.pins.vminit_digest);
        Entry { dir: self.root.join("initfs").join(hex), file: "initfs.ext4", label: "initfs", proof: Proof::Sidecar }
    }

    /// Whether the entry is on disk and intact.
    ///
    /// A digest-pinned entry gets a check on every call. A sidecar entry gets one check for each
    /// process.
    ///
    /// # Errors
    ///
    /// Returns an error if hashing the file fails.
    async fn is_intact(&self, entry: &Entry) -> Result<bool> {
        let path = entry.path();
        if !path.is_file() {
            return Ok(false);
        }
        if matches!(entry.proof, Proof::Sidecar)
            && self.verified.lock().unwrap_or_else(PoisonError::into_inner).contains(&entry.dir)
        {
            return Ok(true);
        }
        let found = hash_file(&path).await?;
        let intact = match &entry.proof {
            Proof::Digest(expected) => found == *expected,
            Proof::Sidecar => fs::read_to_string(sidecar(&path)).is_ok_and(|text| text.trim() == found),
        };
        if intact && matches!(entry.proof, Proof::Sidecar) {
            self.verified.lock().unwrap_or_else(PoisonError::into_inner).insert(entry.dir.clone());
        }
        Ok(intact)
    }

    /// The kernel, fetched if the cache lacks it or holds a damaged copy.
    ///
    /// # Errors
    ///
    /// Returns an error if building the kernel fails or it hashes to something other than its pin.
    pub async fn kernel(&self) -> Result<PathBuf> {
        let entry = Entry {
            dir: self.root.join("kernel").join(self.pins.kernel_sha256),
            file: "vmlinux",
            label: "kernel",
            proof: Proof::Digest(self.pins.kernel_sha256.to_owned()),
        };
        self.ensure(&entry, &Wanted::Kernel).await
    }

    /// The lock for one entry directory.
    fn lock_for(&self, dir: &Path) -> Arc<AsyncMutex<()>> {
        let mut locks: MutexGuard<'_, _> = self.locks.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(dir.to_path_buf()).or_default())
    }

    /// Caches under `root` (the state root's `cache/`), pinned to `pins`.
    #[must_use]
    pub fn new(root: PathBuf, preparer: Arc<dyn Preparer>, pins: &Pins) -> Self {
        Self { locks: Mutex::default(), pins: *pins, preparer, root, verified: Mutex::default() }
    }

    /// The root filesystem of an image, unpacked if the cache lacks it or holds a damaged copy.
    ///
    /// # Errors
    ///
    /// Returns an error if unpacking the layers fails.
    pub async fn rootfs(&self, manifest: &str, layers: &[Layer]) -> Result<PathBuf> {
        ensure!(manifest.bytes().all(|byte| byte.is_ascii_hexdigit()), "{manifest} isn't a digest");
        let entry = Entry {
            dir: self.root.join("rootfs").join(manifest),
            file: "rootfs.ext4",
            label: "rootfs",
            proof: Proof::Sidecar,
        };
        self.ensure(&entry, &Wanted::Rootfs(layers)).await
    }
}

/// The sidecar file for a file's sha256.
fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".sha256");
    PathBuf::from(name)
}

/// The sha256 of a file, hashed off the async workers.
///
/// # Errors
///
/// Returns an error if reading the file fails.
async fn hash_file(path: &Path) -> Result<String> {
    let owned = path.to_path_buf();
    Ok(task::spawn_blocking(move || digest::file(&owned)).await??)
}

/// Records the file's sha256 next to it.
///
/// # Errors
///
/// Returns an error if hashing the file or writing the sidecar fails.
async fn write_sidecar(path: &Path) -> Result<()> {
    let found = hash_file(path).await?;
    fs::write(sidecar(path), format!("{found}\n"))?;
    Ok(())
}
