//! Unit tests for the image import, against archives the tests build.

use super::{Imported, import};
use crate::runtime::containerization::digest::copy_hashing;
use anyhow::{Context as _, Result, ensure};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use tar::{Builder, Header};

/// The `gzip` layer media type.
const LAYER: &str = "application/vnd.oci.image.layer.v1.tar+gzip";

/// A member of a tar archive: its path and its bytes.
type Member = (String, Vec<u8>);

/// The members of a small image and its entry in an index.
type Built = (Vec<Member>, Value);

/// The SHA-256 of `bytes`.
///
/// # Errors
///
/// Fails when hashing fails, which an in-memory source can't cause.
fn sha(bytes: &[u8]) -> Result<String> {
    Ok(copy_hashing(&mut io::Cursor::new(bytes), &mut io::sink())?)
}

/// Writes a tar archive of `(path, bytes)` members.
///
/// # Errors
///
/// Fails when writing the archive fails.
fn write_archive(path: &Path, members: &[Member]) -> Result<()> {
    let mut builder = Builder::new(File::create(path)?);
    for (name, bytes) in members {
        let mut header = Header::new_gnu();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        builder.append_data(&mut header, name, bytes.as_slice())?;
    }
    builder.finish()?;
    Ok(())
}

/// A blob member, named by its digest.
///
/// # Errors
///
/// Fails when hashing fails.
fn blob(bytes: &[u8]) -> Result<Member> {
    Ok((format!("blobs/sha256/{}", sha(bytes)?), bytes.to_vec()))
}

/// The pieces of a small image for platform `architecture`, and its index entry.
///
/// # Errors
///
/// Fails when hashing or serializing fails.
fn image(architecture: &str, layer: &[u8]) -> Result<Built> {
    let config = serde_json::to_vec(&json!({ "architecture": architecture, "os": "linux" }))?;
    let manifest = serde_json::to_vec(&json!({
        "config": { "digest": format!("sha256:{}", sha(&config)?), "mediaType": "application/vnd.oci.image.config.v1+json" },
        "layers": [{ "digest": format!("sha256:{}", sha(layer)?), "mediaType": LAYER }],
    }))?;
    let entry = json!({
        "digest": format!("sha256:{}", sha(&manifest)?),
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "platform": { "architecture": architecture, "os": "linux" },
    });
    Ok((vec![blob(layer)?, blob(&config)?, blob(&manifest)?], entry))
}

/// An archive with an `arm64` image and an `amd64` one, and the `arm64` layer's bytes.
///
/// # Errors
///
/// Fails when building the archive fails.
fn two_platforms(dir: &Path) -> Result<PathBuf> {
    let (mut members, arm) = image("arm64", b"arm layer")?;
    let (others, amd) = image("amd64", b"amd layer")?;
    members.extend(others);
    members.push(("index.json".to_owned(), serde_json::to_vec(&json!({ "manifests": [amd, arm] }))?));
    let archive = dir.join("image.tar");
    write_archive(&archive, &members)?;
    Ok(archive)
}

/// The import finds the `arm64` image and verifies its blobs.
///
/// # Errors
///
/// Fails when the import picks another image or loses a blob.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn the_import_selects_the_arm64_image_and_stores_its_blobs() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let images = dir.path().join("images");
    let imported = import(&two_platforms(dir.path())?, &images).await?;
    ensure!(imported.layers.len() == 1, "{} layers", imported.layers.len());
    let layer = imported.layers.first().context("a layer")?;
    ensure!(layer.media_type == LAYER, "the media type is {}", layer.media_type);
    ensure!(fs::read(&layer.path)? == b"arm layer", "the layer holds other bytes");
    ensure!(fs::read_to_string(&imported.config)?.contains("arm64"), "the config isn't the arm64 one");
    ensure!(imported.config.starts_with(images.join("blobs").join("sha256")), "the blob is outside images/");
    Ok(())
}

/// The import refuses a blob whose bytes hash to another name and removes its temporary file.
///
/// # Errors
///
/// Fails when the import accepts the archive or keeps the blob.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_tampered_blob_is_refused() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let forged = format!("blobs/sha256/{}", sha(b"what the name promises")?);
    let archive = dir.path().join("image.tar");
    write_archive(
        &archive,
        &[(forged.clone(), b"other bytes".to_vec()), ("index.json".to_owned(), b"{\"manifests\":[]}".to_vec())],
    )?;
    let images = dir.path().join("images");
    let err = import(&archive, &images).await.err().context("the import accepted a forged blob")?;
    ensure!(err.to_string().contains("hashes to"), "the error is {err:#}");
    let stored = fs::read_dir(images.join("blobs").join("sha256"))?.count();
    ensure!(stored == 0, "{stored} files were left in blobs/sha256");
    Ok(())
}

/// An unchanged archive skips the second import, and a changed one runs again.
///
/// # Errors
///
/// Fails when the second import reads the archive or the third doesn't.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_unchanged_archive_is_not_imported_twice() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let images = dir.path().join("images");
    let archive = two_platforms(dir.path())?;
    let first = import(&archive, &images).await?;
    fs::remove_file(&first.config)?;
    let second = import(&archive, &images).await?;
    ensure!(second == first, "the memo differs from the first import");
    ensure!(!first.config.exists(), "the second import wrote a blob");
    write_archive(&archive, &[("index.json".to_owned(), b"{\"manifests\":[]}".to_vec())])?;
    ensure!(import(&archive, &images).await.is_err(), "a changed archive reused the memo");
    Ok(())
}

/// An archive with no `arm64` image fails the import.
///
/// # Errors
///
/// Fails when the import succeeds.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn an_archive_without_an_arm64_image_fails() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut members, amd) = image("amd64", b"amd layer")?;
    members.push(("index.json".to_owned(), serde_json::to_vec(&json!({ "manifests": [amd] }))?));
    let archive = dir.path().join("image.tar");
    write_archive(&archive, &members)?;
    ensure!(import(&archive, &dir.path().join("images")).await.is_err(), "the import found an arm64 image");
    Ok(())
}

/// The import follows a nested index to the `arm64` manifest.
///
/// # Errors
///
/// Fails when the import misses the manifest.
///
/// # Panics
///
/// Panics if the async runtime fails to start.
#[tokio::test]
async fn a_nested_index_is_followed() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let (mut members, arm) = image("arm64", b"arm layer")?;
    let inner = serde_json::to_vec(&json!({ "manifests": [arm] }))?;
    let outer = json!({ "manifests": [{
        "digest": format!("sha256:{}", sha(&inner)?),
        "mediaType": "application/vnd.oci.image.index.v1+json",
    }] });
    members.push(blob(&inner)?);
    members.push(("index.json".to_owned(), serde_json::to_vec(&outer)?));
    let archive = dir.path().join("image.tar");
    write_archive(&archive, &members)?;
    let imported: Imported = import(&archive, &dir.path().join("images")).await?;
    ensure!(imported.layers.len() == 1, "{} layers", imported.layers.len());
    Ok(())
}
