use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
pub struct ImageGeneration {
    pub image: PathBuf,
    pub manifest: PathBuf,
    pub current: bool,
}

pub fn manifest_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.manifest.json", image.display()))
}

pub fn current_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.current", image.display()))
}

pub fn generations_path(image: &Path) -> PathBuf {
    PathBuf::from(format!("{}.generations", image.display()))
}

pub fn resolve(image: &Path) -> Result<ImageGeneration, String> {
    let current = current_path(image);
    let metadata = match fs::symlink_metadata(&current) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ImageGeneration {
                image: image.to_path_buf(),
                manifest: manifest_path(image),
                current: false,
            });
        }
        Err(error) => {
            return Err(format!(
                "inspect image generation {}: {error}",
                current.display()
            ))
        }
    };
    if !metadata.file_type().is_symlink() {
        return Err(format!(
            "image generation pointer is not a symbolic link: {}",
            current.display()
        ));
    }

    let directory = fs::canonicalize(&current)
        .map_err(|error| format!("resolve image generation {}: {error}", current.display()))?;
    let generations = fs::canonicalize(generations_path(image)).map_err(|error| {
        format!(
            "resolve image generations directory for {}: {error}",
            image.display()
        )
    })?;
    if !directory.is_dir() || directory.parent() != Some(generations.as_path()) {
        return Err(format!(
            "image generation pointer does not resolve to a directory directly below {}: {}",
            generations.display(),
            current.display()
        ));
    }
    let name = image
        .file_name()
        .ok_or_else(|| "image path has no file name".to_owned())?;
    let resolved_image = directory.join(name);
    Ok(ImageGeneration {
        manifest: manifest_path(&resolved_image),
        image: resolved_image,
        current: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn uses_configured_paths_before_first_generation() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("retained.qcow2");

        assert_eq!(
            resolve(&image).unwrap(),
            ImageGeneration {
                manifest: manifest_path(&image),
                image,
                current: false,
            }
        );
    }

    #[test]
    fn resolves_one_current_generation() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("retained.qcow2");
        let generations = generations_path(&image);
        let generation = generations.join("one");
        fs::create_dir_all(&generation).unwrap();
        symlink("retained.qcow2.generations/one", current_path(&image)).unwrap();

        assert_eq!(
            resolve(&image).unwrap(),
            ImageGeneration {
                image: generation.join("retained.qcow2"),
                manifest: generation.join("retained.qcow2.manifest.json"),
                current: true,
            }
        );
    }

    #[test]
    fn rejects_dangling_and_external_current_pointers() {
        let directory = tempfile::tempdir().unwrap();
        let image = directory.path().join("retained.qcow2");
        symlink("missing", current_path(&image)).unwrap();
        assert!(resolve(&image).is_err());

        fs::remove_file(current_path(&image)).unwrap();
        let external = directory.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::create_dir(generations_path(&image)).unwrap();
        symlink(&external, current_path(&image)).unwrap();
        assert!(resolve(&image).is_err());
    }
}
