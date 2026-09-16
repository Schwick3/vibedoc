//! User-owned adapter registry. Reading this module never downloads or executes code.
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
};

pub const NAMES: [&str; 2] = ["python", "typescript"];

pub fn valid_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

pub fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn relative_path(value: &Path) -> bool {
    !value.as_os_str().is_empty()
        && value
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

pub fn home() -> io::Result<PathBuf> {
    let path = if let Some(value) = env::var_os("VIBEDOC_ADAPTER_HOME") {
        PathBuf::from(value)
    } else if cfg!(target_os = "macos") {
        user_home()?.join("Library/Application Support/vibedoc/adapters")
    } else if cfg!(target_os = "linux") {
        let data = match env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            Some(value) => PathBuf::from(value),
            None => user_home()?.join(".local/share"),
        };
        data.join("vibedoc/adapters")
    } else {
        return Err(io::Error::other(
            "managed adapters support macOS and Linux only",
        ));
    };
    if !path.is_absolute() {
        return Err(io::Error::other(
            "adapter data directory must be absolute (check VIBEDOC_ADAPTER_HOME/XDG_DATA_HOME)",
        ));
    }
    Ok(path)
}

fn user_home() -> io::Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| io::Error::other("HOME must be an absolute path"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub source: String,
}

impl Record {
    pub fn validate(&self, name: &str) -> io::Result<()> {
        if self.schema_version != 1
            || self.name != name
            || !NAMES.contains(&name)
            || !valid_version(&self.version)
            || !valid_digest(&self.sha256)
            || !relative_path(&self.directory)
            || self.directory.components().count() != 2
            || !self.directory.starts_with("versions")
            || !relative_path(&self.executable)
        {
            return Err(io::Error::other(format!(
                "invalid managed metadata for {name}; remove and reinstall the adapter"
            )));
        }
        Ok(())
    }
}

pub fn read_record(path: &Path, name: &str) -> io::Result<Option<Record>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(io::Error::other(format!(
            "invalid managed metadata at {}; remove and reinstall {name}",
            path.display()
        )));
    }
    let bytes = fs::read(path)?;
    let record: Record = serde_json::from_slice(&bytes).map_err(|e| {
        io::Error::other(format!(
            "invalid managed metadata at {}: {e}; remove and reinstall {name}",
            path.display()
        ))
    })?;
    record.validate(name)?;
    Ok(Some(record))
}

pub fn resolve(root: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    let Some(record) = read_record(&root.join(name).join("active.json"), name)? else {
        return Ok(None);
    };
    let directory = root.join(name).join(&record.directory).canonicalize()?;
    let executable = directory.join(&record.executable).canonicalize()?;
    if !directory.starts_with(root.join(name).canonicalize()?)
        || !executable.starts_with(&directory)
        || !crate::adapter::is_executable(&executable)
    {
        return Err(io::Error::other(format!(
            "invalid managed executable for {name}; reinstall the adapter"
        )));
    }
    Ok(Some(executable))
}

pub fn on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|p| p.join(format!("vibedoc-adapter-{name}")))
            .find(|p| crate::adapter::is_executable(p))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_records_never_look_like_missing_installations() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("python");
        fs::create_dir_all(&base).unwrap();
        let active = base.join("active.json");
        assert!(resolve(tmp.path(), "python").unwrap().is_none());
        fs::write(&active, "{}").unwrap();
        assert!(resolve(tmp.path(), "python").is_err());
        let mut record = Record {
            schema_version: 1,
            name: "python".into(),
            version: "1.0.0".into(),
            sha256: "a".repeat(64),
            directory: "versions/install".into(),
            executable: "../escape".into(),
            source: "fixture".into(),
        };
        fs::write(&active, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(resolve(tmp.path(), "python").is_err());
        record.executable = "adapter".into();
        record.directory = "../outside".into();
        fs::write(&active, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(resolve(tmp.path(), "python").is_err());
        fs::remove_file(&active).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing.json", &active).unwrap();
            assert!(resolve(tmp.path(), "python").is_err());
        }
    }
}
