use reqwest::{self, StatusCode};
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Result};
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zip;

/// Unzip `zip_file` in `dir`, ensure that all paths start with `dragon_tiger/`,
/// and return the path to the `dragon_tiger` directory.
pub fn unzip(dir: &PathBuf, zip_file: &str) -> Result<PathBuf> {
    if zip_file.starts_with("http://") || zip_file.starts_with("https://") {
        return unzip_url(dir, zip_file);
    }
    let reader = File::open(zip_file)?;
    let mut zip = zip::ZipArchive::new(reader)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_owned();
        let name = Path::new(&name);
        if name.is_absolute() || !name.starts_with("dragon-tiger/") {
            return Err(Error::new(
                ErrorKind::Other,
                format!(
                    "file name in zip does not start with dragon-tiger/: {:?}",
                    name
                ),
            ));
        }
        let target_file = dir.join(name);
        if target_file.to_str().unwrap().ends_with('/') {
            fs::create_dir(&target_file)?;
        } else {
            let mut target = File::create(target_file)?;
            let mut content = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut content)?;
            target.write_all(&content)?;
            let mut perms = target.metadata()?.permissions();
            perms.set_mode(file.unix_mode().unwrap_or(0o600));
            target.set_permissions(perms)?;
        }
    }
    Ok(dir.join("dragon-tiger"))
}

fn unzip_url(dir: &PathBuf, url: &str) -> Result<PathBuf> {
    let mut zip = reqwest::get(url)
        .map_err(|e| Error::new(ErrorKind::Other, format!("cannot retrieve {}: {}", url, e)))?;
    if zip.status() != StatusCode::Ok {
        return Err(Error::new(ErrorKind::Other, format!("cannot retrieve {}: {}", url, zip.status())));
    }
    let target_file = dir.join("dragon-tiger.zip");
    let mut target = File::create(&target_file)?;
    zip.copy_to(&mut target).map_err(|e| Error::new(ErrorKind::Other, format!("cannot write zip file: {}", e)))?;
    unzip(dir, target_file.to_str().unwrap())
}
