//! Imports an OCI image layout archive into `images/`, verifying every blob on the way in.
//!
//! The archive is what `container image save` writes. A blob appears at `blobs/sha256/<hex>` only
//! after its bytes hash to `<hex>`, so avirthostd verified every blob that it hands to avirtvmd.

#[cfg(test)]
mod tests;

use super::digest::copy_hashing;
use super::temp::{PREFIX, Temp};
use crate::state::private_dir;
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tar::Archive;
use tokio::task;

/// The largest `index.json` or manifest that the import reads.
const MAX_JSON: u64 = 1 << 24;
/// How many levels of nested indexes the import follows.
const MAX_INDEX_DEPTH: u8 = 4;

/// One layer of an imported image.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Layer {
    /// The OCI media type, which tells the layer's compression.
    pub media_type: String,
    /// The layer's blob.
    pub path: PathBuf,
}

/// What an import found for the `linux/arm64` image.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Imported {
    /// The image config blob.
    pub config: PathBuf,
    /// The layers, bottom first.
    pub layers: Vec<Layer>,
    /// The hexadecimal digest of the image manifest.
    pub manifest: String,
}

/// An archive's size and modification time, which say whether an earlier import still applies.
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Stamp {
    /// The modification time's nanosecond part.
    nanos: u32,
    /// The modification time in whole seconds since the epoch.
    seconds: u64,
    /// The size in bytes.
    size: u64,
}

/// The record of an import under `images/refs/`.
#[derive(Debug, Deserialize, Serialize)]
struct Memo {
    /// What the import found.
    imported: Imported,
    /// The archive the import read.
    stamp: Stamp,
}

/// A descriptor in an index or a manifest.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Descriptor {
    /// `sha256:<hex>`.
    digest: String,
    /// The OCI media type.
    media_type: String,
    /// The platform of a manifest in an index.
    platform: Option<Platform>,
}

/// A platform in an index.
#[derive(Debug, Deserialize)]
struct Platform {
    /// `arm64` for the images avirt runs.
    architecture: String,
    /// `linux` for the images avirt runs.
    os: String,
}

/// An index, which lists manifests or further indexes.
#[derive(Debug, Deserialize)]
struct Index {
    /// The descriptors.
    manifests: Vec<Descriptor>,
}

/// An image manifest.
#[derive(Debug, Deserialize)]
struct Manifest {
    /// The image config.
    config: Descriptor,
    /// The layers, bottom first.
    layers: Vec<Descriptor>,
}

/// Imports the archive into `images_dir` and returns the `linux/arm64` image's blobs.
///
/// Importing an archive of the same size and modification time again reads nothing from it.
///
/// # Errors
///
/// Returns an error if reading the archive fails, a blob doesn't hash to its name, or the archive
/// has no `linux/arm64` image.
pub async fn import(archive: &Path, images_dir: &Path) -> Result<Imported> {
    let (source, target) = (archive.to_path_buf(), images_dir.to_path_buf());
    task::spawn_blocking(move || import_blocking(&source, &target)).await?
}

/// [`import`] on the calling thread.
///
/// # Errors
///
/// Returns an error under the same conditions as [`import`].
fn import_blocking(archive: &Path, images_dir: &Path) -> Result<Imported> {
    let stamp = stamp_of(archive)?;
    let blobs = images_dir.join("blobs").join("sha256");
    let refs = images_dir.join("refs");
    private_dir(&blobs)?;
    private_dir(&refs)?;
    let name = archive.file_name().context("the archive has no file name")?;
    let memo_path = refs.join(format!("{}.json", name.to_string_lossy()));
    if let Some(memo) = read_memo(&memo_path)
        && memo.stamp == stamp
    {
        return Ok(memo.imported);
    }
    let index = store_blobs(archive, &blobs)?;
    let manifest = select_manifest(&index, &blobs, MAX_INDEX_DEPTH)?;
    let imported = describe(&manifest, &blobs)?;
    let memo = Memo { imported: imported.clone(), stamp };
    fs::write(&memo_path, serde_json::to_vec(&memo)?)?;
    Ok(imported)
}

/// The archive's size and modification time.
///
/// # Errors
///
/// Returns an error if examining the archive fails.
fn stamp_of(archive: &Path) -> Result<Stamp> {
    let meta = fs::metadata(archive).with_context(|| format!("stat {}", archive.display()))?;
    let since_epoch = meta.modified()?.duration_since(UNIX_EPOCH)?;
    Ok(Stamp { nanos: since_epoch.subsec_nanos(), seconds: since_epoch.as_secs(), size: meta.len() })
}

/// The memo, if it reads. A damaged memo means the import runs again.
fn read_memo(path: &Path) -> Option<Memo> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// Writes every blob in the archive to `blobs` after checking its hash, and returns `index.json`.
///
/// # Errors
///
/// Returns an error if the archive is unreadable, a blob fails its hash, or `index.json` is missing.
fn store_blobs(archive: &Path, blobs: &Path) -> Result<Vec<u8>> {
    let mut index = None;
    let mut tar = Archive::new(BufReader::new(File::open(archive)?));
    for item in tar.entries()? {
        let mut entry = item?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().trim_start_matches("./").to_owned();
        if path == "index.json" {
            let mut bytes = Vec::new();
            entry.by_ref().take(MAX_JSON).read_to_end(&mut bytes)?;
            index = Some(bytes);
        } else if let Some(name) = path.strip_prefix("blobs/sha256/") {
            store_blob(&mut entry, name, blobs)?;
        } else {
            // Directories, `oci-layout`, and anything else the import doesn't need.
        }
    }
    index.context("the archive has no index.json")
}

/// Whether `text` is 64 lowercase hexadecimal characters.
fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Writes one blob to `blobs/<name>`, replacing a blob already there, if its bytes hash to `name`.
///
/// # Errors
///
/// Returns an error if `name` isn't a digest, the copy fails, or the bytes hash to something else.
fn store_blob<R>(source: &mut R, name: &str, blobs: &Path) -> Result<()>
where
    R: Read,
{
    ensure!(is_sha256_hex(name), "the archive holds a blob named {name}, which isn't a sha256 digest");
    let temp = Temp::new(blobs.join(format!("{PREFIX}{:016x}", rand::random::<u64>())));
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(temp.path())?;
    let computed = copy_hashing(source, &mut file)?;
    drop(file);
    ensure!(computed == name, "blob {name} hashes to {computed}");
    temp.persist(&blobs.join(name))?;
    Ok(())
}

/// The path of a verified blob that a descriptor names.
///
/// # Errors
///
/// Returns an error if the digest isn't a sha256 one or the blob isn't in the archive.
fn blob_path(descriptor: &Descriptor, blobs: &Path) -> Result<PathBuf> {
    let hex = descriptor.digest.strip_prefix("sha256:").context("only sha256 digests are supported")?;
    ensure!(is_sha256_hex(hex), "{} isn't a sha256 digest", descriptor.digest);
    let path = blobs.join(hex);
    ensure!(path.is_file(), "the archive lacks blob {hex}");
    Ok(path)
}

/// Reads a JSON blob.
///
/// # Errors
///
/// Returns an error if the file is unreadable or isn't the JSON that `T` describes.
fn read_json<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let mut bytes = Vec::new();
    File::open(path)?.take(MAX_JSON).read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Whether a descriptor is a `linux/arm64` image. A manifest without a platform counts.
fn is_linux_arm64(descriptor: &Descriptor) -> bool {
    descriptor.platform.as_ref().is_none_or(|platform| platform.os == "linux" && platform.architecture == "arm64")
}

/// Whether a descriptor is another index.
fn is_index(descriptor: &Descriptor) -> bool {
    descriptor.media_type.ends_with(".index.v1+json") || descriptor.media_type.ends_with(".manifest.list.v2+json")
}

/// Finds the `linux/arm64` manifest in an index, following nested indexes.
///
/// # Errors
///
/// Returns an error for an index that nests too deeply or doesn't parse, and for a missing image.
fn select_manifest(index_json: &[u8], blobs: &Path, depth: u8) -> Result<PathBuf> {
    ensure!(depth > 0, "the archive nests its indexes too deeply");
    let index: Index = serde_json::from_slice(index_json)?;
    for descriptor in index.manifests.iter().filter(|descriptor| is_linux_arm64(descriptor)) {
        let path = blob_path(descriptor, blobs)?;
        if is_index(descriptor) {
            if let Ok(found) = select_manifest(&fs::read(&path)?, blobs, depth.saturating_sub(1)) {
                return Ok(found);
            }
        } else {
            return Ok(path);
        }
    }
    bail!("the archive has no linux/arm64 image")
}

/// The config and layer blobs of the manifest at `manifest`.
///
/// # Errors
///
/// Returns an error for a malformed manifest or a blob that the archive omits.
fn describe(manifest: &Path, blobs: &Path) -> Result<Imported> {
    let parsed: Manifest = read_json(manifest)?;
    let mut layers = Vec::new();
    for layer in &parsed.layers {
        layers.push(Layer { media_type: layer.media_type.clone(), path: blob_path(layer, blobs)? });
    }
    let name = manifest.file_name().context("the manifest has no file name")?;
    Ok(Imported { config: blob_path(&parsed.config, blobs)?, layers, manifest: name.to_string_lossy().into_owned() })
}
