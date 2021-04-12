// TODO This should use asynchronous file operations, as well as run zip/unzip
// operations onto a worker thread.
use failure::{bail, ensure, ResultExt};
use reqwest::{self, StatusCode};
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use zip::write::{FileOptions, ZipWriter};

/// Unzip `zip_file` in `dir`, ensure that all paths start with the specified
/// `prefix` directory and a slash and return the path to this directory.
/// `zip_file` may be an URL starting with `http://` or `https://`.
pub async fn unzip(dir: &Path, zip_file: &str, prefix: &str) -> Result<PathBuf, failure::Error> {
    if zip_file.starts_with("http://") || zip_file.starts_with("https://") {
        let downloaded_file = download_url(dir, zip_file, prefix).await?;
        unzip_file(dir, downloaded_file.to_str().unwrap(), prefix)
    } else {
        unzip_file(dir, zip_file, prefix)
    }
}

fn unzip_file(dir: &Path, zip_file: &str, prefix: &str) -> Result<PathBuf, failure::Error> {
    let with_slash = format!("{}/", prefix);
    let reader = File::open(zip_file)?;
    let mut zip = zip::ZipArchive::new(reader)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();
        let name = Path::new(&name);
        ensure!(
            !name.is_absolute() && name.starts_with(&with_slash),
            "file name in zip does not start with {}: {:?}",
            with_slash,
            name
        );
        let target_file = dir.join(name);
        if target_file.to_str().unwrap().ends_with('/') {
            fs::create_dir(&target_file)?;
        } else {
            let mut target = File::create(target_file)?;
            io::copy(&mut file, &mut target)?;
            let mut perms = target.metadata()?.permissions();
            perms.set_mode(file.unix_mode().unwrap_or(0o600));
            target.set_permissions(perms)?;
        }
    }
    let dir = dir.join(prefix);
    info!("unzipped compiler is available in {:?}", dir);
    Ok(dir)
}

async fn download_url(dir: &Path, url: &str, prefix: &str) -> Result<PathBuf, failure::Error> {
    let zip = reqwest::get(url)
        .await
        .with_context(|e| format!("cannot retrieve {}: {}", url, e))?;
    if zip.status() != StatusCode::OK {
        warn!("could not retrieve {}: {}", url, zip.status());
        bail!("cannot retrieve {}: {}", url, zip.status());
    }
    let target_file = dir.join(&format!("{}.zip", prefix));
    debug!("retrieving {} as {:?}", url, target_file);
    let mut target = tokio::fs::File::create(&target_file).await?;
    // TODO Use bytes_stream() and progressive write to avoid loading the whole file to memory.
    target
        .write_all(&zip.bytes().await?[..])
        .await
        .with_context(|e| format!("cannot write zip file: {}", e))?;
    target.sync_all().await?;
    Ok(target_file)
}

/// Recursively build zip while setting the top level name
pub fn zip_recursive(dir: &Path, top_level: &Path, zip_file: &Path) -> Result<(), io::Error> {
    let writer = File::create(zip_file)?;
    let mut zip = ZipWriter::new(writer);
    add_to_zip(&mut zip, &dir.to_owned(), &Path::new(top_level).to_owned())
}

fn add_to_zip(
    zip_file: &mut ZipWriter<File>,
    dir: &Path,
    dir_in_zip: &Path,
) -> Result<(), io::Error> {
    zip_file.add_directory(
        format!("{}/", dir_in_zip.to_string_lossy()),
        FileOptions::default(),
    )?;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let full_path = dir.join(&path);
        let metadata = full_path.metadata()?;
        let file_name = path.file_name().unwrap().to_string_lossy();
        let zip_path = dir_in_zip.join(&*file_name);
        if metadata.is_dir() {
            add_to_zip(zip_file, &full_path, &zip_path)?;
        } else if metadata.is_file() {
            let mode = metadata.permissions().mode();
            zip_file.start_file(
                zip_path.to_string_lossy(),
                FileOptions::default().unix_permissions(mode),
            )?;
            io::copy(&mut File::open(&full_path)?, zip_file)?;
        }
    }
    Ok(())
}
