//! Unit tests for the guest artifact caches, with a fake preparer.

use super::{Artifacts, Pins, Preparer, ROOTFS_CAPACITY};
use crate::runtime::containerization::digest::copy_hashing;
use crate::runtime::containerization::images::Layer;
use alloc::sync::Arc;
use anyhow::{Result, ensure};
use async_trait::async_trait;
use core::future;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tokio::time;

/// The tarball that the fake serves.
const TARBALL: &[u8] = b"kernel tarball bytes";
/// The kernel inside that tarball.
const KERNEL: &[u8] = b"kernel image bytes";
/// The init filesystem that the fake writes.
const INITFS: &[u8] = b"init filesystem bytes";
/// The init image's digest, as the fake reports it.
const INIT_DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A preparer that writes fixed bytes and counts its calls.
#[derive(Default)]
struct Fake {
    /// How many more kernel extractions write the wrong bytes.
    bad_kernels: AtomicUsize,
    /// How many fetches happened.
    fetches: AtomicUsize,
    /// How many root filesystem builds happened.
    rootfs_builds: AtomicUsize,
    /// How many fetches never finish.
    stalled_fetches: AtomicUsize,
    /// The digest that `initfs` reports.
    wrong_init_digest: bool,
}

#[async_trait]
impl Preparer for Fake {
    async fn extract(&self, _archive: &Path, _member: &str, out: &Path) -> Result<()> {
        let bad = self.bad_kernels.load(Ordering::SeqCst) > 0;
        if bad {
            self.bad_kernels.fetch_sub(1, Ordering::SeqCst);
        }
        fs::write(out, if bad { b"not the kernel".as_slice() } else { KERNEL })?;
        Ok(())
    }

    async fn fetch(&self, _url: &str, out: &Path) -> Result<()> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        fs::write(out, TARBALL)?;
        if self.stalled_fetches.load(Ordering::SeqCst) > 0 {
            future::pending::<()>().await;
        }
        Ok(())
    }

    async fn initfs(&self, _reference: &str, store: &Path, out: &Path) -> Result<String> {
        fs::create_dir_all(store)?;
        fs::write(out, INITFS)?;
        Ok(if self.wrong_init_digest { "sha256:ffff".to_owned() } else { INIT_DIGEST.to_owned() })
    }

    async fn rootfs(&self, out: &Path, capacity: u64, layers: &[Layer]) -> Result<()> {
        ensure!(capacity == ROOTFS_CAPACITY && layers.len() == 1, "unexpected arguments");
        self.rootfs_builds.fetch_add(1, Ordering::SeqCst);
        fs::write(out, b"a root filesystem")?;
        Ok(())
    }
}

/// The SHA-256 of `bytes`, as hexadecimal digits.
///
/// # Errors
///
/// Fails when hashing fails, which an in-memory source can't cause.
fn sha(bytes: &[u8]) -> Result<String> {
    Ok(copy_hashing(&mut io::Cursor::new(bytes), &mut io::sink())?)
}

/// Pins that match the fake's bytes. The strings last as long as the process.
///
/// # Errors
///
/// Fails when hashing fails.
fn pins() -> Result<Pins> {
    Ok(Pins {
        kernel_member: "vmlinux",
        kernel_sha256: Box::leak(sha(KERNEL)?.into_boxed_str()),
        kernel_url: "https://example.invalid/kernel.tar.zst",
        tarball_sha256: Box::leak(sha(TARBALL)?.into_boxed_str()),
        vminit_digest: INIT_DIGEST,
        vminit_reference: "example.invalid/vminit:1",
    })
}

/// Caches in a temporary directory over `fake`.
///
/// # Errors
///
/// Fails when making the pins or the directory fails.
fn artifacts(fake: &Arc<Fake>) -> Result<(Artifacts, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let preparer: Arc<dyn Preparer> = Arc::<Fake>::clone(fake);
    Ok((Artifacts::new(dir.path().join("cache"), preparer, &pins()?), dir))
}

/// The layers of a one-layer image.
fn layers() -> Vec<Layer> {
    vec![Layer { media_type: "application/vnd.oci.image.layer.v1.tar".to_owned(), path: PathBuf::from("/layer") }]
}

/// The cache fetches the kernel once and then serves it.
///
/// # Errors
///
/// Fails when a second fetch happens or the bytes differ.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn the_kernel_is_fetched_once_and_then_cached() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, _dir) = artifacts(&fake)?;
    let first = cache.kernel().await?;
    let second = cache.kernel().await?;
    ensure!(first == second && fs::read(&first)? == KERNEL, "the cache returned other bytes");
    ensure!(fake.fetches.load(Ordering::SeqCst) == 1, "{} fetches", fake.fetches.load(Ordering::SeqCst));
    ensure!(!first.with_file_name("kernel.tar.zst").exists(), "the tarball stayed in the cache");
    Ok(())
}

/// The cache refuses a kernel with the wrong digest, and the next call fetches again.
///
/// # Errors
///
/// Fails when the cache accepts the wrong kernel or a retry doesn't recover.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_kernel_with_the_wrong_digest_is_refused_and_fetched_again() -> Result<()> {
    let fake = Arc::new(Fake { bad_kernels: AtomicUsize::new(1), ..Fake::default() });
    let (cache, _dir) = artifacts(&fake)?;
    let refused = cache.kernel().await.err().map(|err| format!("{err:#}"));
    ensure!(refused.is_some_and(|text| text.contains("hashes to")), "the wrong kernel was accepted");
    let path = cache.kernel().await?;
    ensure!(fs::read(path)? == KERNEL, "the retry stored other bytes");
    ensure!(fake.fetches.load(Ordering::SeqCst) == 2, "{} fetches", fake.fetches.load(Ordering::SeqCst));
    Ok(())
}

/// A second wrong kernel in a row fails the launch.
///
/// # Errors
///
/// Fails when the cache accepts a kernel that never matches.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_kernel_that_never_matches_fails() -> Result<()> {
    let fake = Arc::new(Fake { bad_kernels: AtomicUsize::new(usize::MAX), ..Fake::default() });
    let (cache, _dir) = artifacts(&fake)?;
    ensure!(cache.kernel().await.is_err(), "the first call accepted a wrong kernel");
    ensure!(cache.kernel().await.is_err(), "the second call accepted a wrong kernel");
    Ok(())
}

/// The cache notices a cached kernel that changed on disk and builds it again.
///
/// # Errors
///
/// Fails when the cache serves the damaged file or keeps it.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_damaged_cached_kernel_is_detected_and_rebuilt() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, _dir) = artifacts(&fake)?;
    let path = cache.kernel().await?;
    fs::write(&path, b"a kernel plus one byte!")?;
    ensure!(cache.kernel().await? == path && fs::read(&path)? == KERNEL, "the damaged kernel was served");
    ensure!(fake.fetches.load(Ordering::SeqCst) == 2, "{} fetches", fake.fetches.load(Ordering::SeqCst));
    Ok(())
}

/// Concurrent launches of one entry build it once.
///
/// # Errors
///
/// Fails when a second fetch happens.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn concurrent_launches_build_an_entry_once() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, _dir) = artifacts(&fake)?;
    let (left, right) = tokio::join!(cache.kernel(), cache.kernel());
    ensure!(left? == right?, "the launches got different entries");
    ensure!(fake.fetches.load(Ordering::SeqCst) == 1, "{} fetches", fake.fetches.load(Ordering::SeqCst));
    Ok(())
}

/// The init filesystem comes with a sidecar and gets one check for each process. A new process
/// rebuilds it after damage.
///
/// # Errors
///
/// Fails when the sidecar is missing, the check repeats in one process, or a new process misses damage.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn the_init_filesystem_is_checked_once_for_each_process() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, dir) = artifacts(&fake)?;
    let path = cache.initfs().await?;
    ensure!(fs::read(&path)? == INITFS, "the init filesystem holds other bytes");
    ensure!(path.with_file_name("initfs.ext4.sha256").is_file(), "the sidecar is missing");
    ensure!(!path.with_file_name("store").exists(), "the image store stayed in the cache");
    fs::write(&path, b"damaged")?;
    ensure!(fs::read(cache.initfs().await?)? == b"damaged", "one process checked the sidecar twice");
    let preparer: Arc<dyn Preparer> = Arc::<Fake>::clone(&fake);
    let fresh = Artifacts::new(dir.path().join("cache"), preparer, &pins()?);
    ensure!(fs::read(fresh.initfs().await?)? == INITFS, "a new process missed the damage");
    Ok(())
}

/// The cache refuses an init image other than the pinned one.
///
/// # Errors
///
/// Fails when the cache accepts the wrong image or leaves an entry behind.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_init_image_with_another_digest_is_refused() -> Result<()> {
    let fake = Arc::new(Fake { wrong_init_digest: true, ..Fake::default() });
    let (cache, dir) = artifacts(&fake)?;
    ensure!(cache.initfs().await.is_err(), "the wrong init image was accepted");
    let left = fs::read_dir(dir.path().join("cache").join("initfs"))?.count();
    ensure!(left == 0, "{left} entries were left behind");
    Ok(())
}

/// A root filesystem has its manifest as the key and one build.
///
/// # Errors
///
/// Fails when the key fails or a second build happens.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_root_filesystem_is_keyed_by_its_manifest_and_built_once() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, _dir) = artifacts(&fake)?;
    let manifest = "ab".repeat(32);
    let first = cache.rootfs(&manifest, &layers()).await?;
    let second = cache.rootfs(&manifest, &layers()).await?;
    ensure!(
        first == second && first.ends_with(format!("rootfs/{manifest}/rootfs.ext4")),
        "the entry is at {}",
        first.display()
    );
    ensure!(fake.rootfs_builds.load(Ordering::SeqCst) == 1, "the filesystem was built twice");
    ensure!(cache.rootfs("../escape", &layers()).await.is_err(), "a path-like key was accepted");
    Ok(())
}

/// A manifest that isn't a lowercase sha256 digest is refused before the cache touches anything.
///
/// With an empty key the entry directory is the whole root filesystem cache, and a mismatch there would delete every entry.
///
/// # Errors
///
/// Fails when a bad key passes or a cached entry goes.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_manifest_that_is_not_a_lowercase_digest_leaves_the_cache_alone() -> Result<()> {
    let fake = Arc::new(Fake::default());
    let (cache, _dir) = artifacts(&fake)?;
    let manifest = "ab".repeat(32);
    let kept = cache.rootfs(&manifest, &layers()).await?;
    let (upper, short, long) = ("AB".repeat(32), "abab", "ab".repeat(33));
    for bad in ["", upper.as_str(), short, long.as_str()] {
        ensure!(cache.rootfs(bad, &layers()).await.is_err(), "{bad:?} was accepted");
    }
    ensure!(kept.is_file(), "a cached entry was removed");
    Ok(())
}

/// A launch that stops in the middle of a build removes its temporary entry.
///
/// # Errors
///
/// Fails when a `.tmp-` entry remains under the kernel cache.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_cancelled_build_removes_its_temporary_entry() -> Result<()> {
    let fake = Arc::new(Fake { stalled_fetches: AtomicUsize::new(1), ..Fake::default() });
    let (caches, dir) = artifacts(&fake)?;
    let pool = Arc::new(caches);
    let build = tokio::spawn({
        let worker = Arc::clone(&pool);
        async move { worker.kernel().await }
    });
    while fake.fetches.load(Ordering::SeqCst) == 0 {
        time::sleep(Duration::from_millis(5)).await;
    }
    ensure!(fs::read_dir(dir.path().join("cache/kernel"))?.count() == 1, "the build left no temporary entry to test");
    build.abort();
    ensure!(build.await.is_err(), "the build finished");
    let left = fs::read_dir(dir.path().join("cache/kernel"))?.count();
    ensure!(left == 0, "{left} entries remain under the kernel cache");
    Ok(())
}
