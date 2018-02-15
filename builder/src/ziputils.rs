use std::fs::{self, File};
use std::io::{Error, ErrorKind, Result};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use zip;

/// Unzip `zip_file` in `dir`, ensure that all paths start with `dragon_tiger/`,
/// and return the path to the `dragon_tiger` directory.
pub fn unzip(dir: &PathBuf, zip_file: &PathBuf) -> Result<PathBuf> {
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
        }
    }
    Ok(dir.join("dragon-tiger"))
}
