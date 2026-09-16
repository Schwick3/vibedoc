//! Explicit installation only: ordinary project commands never call this module.
use crate::{CliError, OutputFormat, VERSION};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand, ValueEnum};
use flate2::read::GzDecoder;
use fs2::FileExt;
use reqwest::{Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vibedoc_core::{
    AdapterClient, AdapterRunOptions,
    managed::{self, Record},
};

const MANIFEST_LIMIT: u64 = 1024 * 1024;
const ARCHIVE_LIMIT: u64 = 128 * 1024 * 1024;
const EXTRACT_LIMIT: u64 = 512 * 1024 * 1024;
const ENTRY_LIMIT: usize = 20_000;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Name {
    Python,
    Typescript,
}
impl Name {
    fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Typescript => "typescript",
        }
    }
}

#[derive(Args)]
pub struct AdapterArgs {
    #[command(subcommand)]
    command: AdapterCommand,
}
#[derive(Subcommand)]
enum AdapterCommand {
    /// Download, validate, and activate an official adapter.
    Install {
        #[arg(value_enum)]
        name: Name,
        /// Exact stable version (defaults to the CLI version).
        #[arg(long, conflicts_with = "manifest")]
        version: Option<String>,
        /// Explicitly trusted local or HTTPS manifest.
        #[arg(long)]
        manifest: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// List managed versions and PATH fallbacks without executing adapters.
    List {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Remove only this adapter's managed installations and cache.
    Remove {
        #[arg(value_enum)]
        name: Name,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}
impl AdapterArgs {
    pub fn format(&self) -> OutputFormat {
        match self.command {
            AdapterCommand::Install { format, .. }
            | AdapterCommand::List { format }
            | AdapterCommand::Remove { format, .. } => format,
        }
    }
}

pub fn run(args: AdapterArgs) -> Result<u8, CliError> {
    execute(args).map_err(|e| CliError(format!("{e:#}")))
}
fn execute(args: AdapterArgs) -> Result<u8> {
    let format = args.format();
    let root = managed::home()?;
    let output = match args.command {
        AdapterCommand::Install {
            name,
            version,
            manifest,
            ..
        } => {
            let expected = if manifest.is_none() {
                Some(version.unwrap_or_else(|| VERSION.into()))
            } else {
                None
            };
            if let Some(v) = &expected {
                ensure!(
                    managed::valid_version(v),
                    "version must be an exact MAJOR.MINOR.PATCH"
                );
            }
            let source = manifest.unwrap_or_else(|| {
                format!(
                    "https://github.com/Schwick3/vibedoc/releases/download/v{}/adapters.json",
                    expected.as_ref().unwrap()
                )
            });
            let record = install(&root, name.as_str(), &source, expected.as_deref(), &Https::new()?)
                .with_context(|| format!("installing {} from {source}; for unpublished versions use --manifest /absolute/path/adapters.json", name.as_str()))?;
            serde_json::json!({"status":"installed", "adapter": record, "path": managed::resolve(&root, name.as_str())?})
        }
        AdapterCommand::List { .. } => serde_json::json!({"status":"ok", "adapters": list(&root)?}),
        AdapterCommand::Remove { name, .. } => {
            remove(&root, name.as_str())?;
            serde_json::json!({"status":"removed", "adapter": name.as_str()})
        }
    };
    if format == OutputFormat::Json {
        let mut output = output;
        output["schemaVersion"] = 1.into();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if let Some(items) = output.get("adapters").and_then(|v| v.as_array()) {
        for item in items {
            println!(
                "{}: {}",
                item["name"].as_str().unwrap(),
                item["activePath"]
                    .as_str()
                    .unwrap_or("no managed installation")
            );
            for version in item["versions"].as_array().unwrap() {
                println!(
                    "  managed {}{}",
                    version["version"].as_str().unwrap(),
                    if version["active"] == true {
                        " (active)"
                    } else {
                        ""
                    }
                );
            }
            if let Some(path) = item["pathFallback"].as_str() {
                println!(
                    "  PATH: {path}{}",
                    if item["shadowed"] == true {
                        " (shadowed)"
                    } else {
                        ""
                    }
                );
            }
        }
    } else if output["status"] == "installed" {
        println!(
            "Installed {} {} at {}",
            output["adapter"]["name"].as_str().unwrap(),
            output["adapter"]["version"].as_str().unwrap(),
            output["path"].as_str().unwrap()
        );
    } else {
        println!(
            "Removed managed installations for {}",
            output["adapter"].as_str().unwrap()
        );
    }
    Ok(0)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    adapters: Vec<Entry>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    version: String,
    protocol_version: u32,
    runtime: Runtime,
    archive: String,
    sha256: String,
    executable: PathBuf,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Runtime {
    name: String,
    min_version: String,
}

impl Manifest {
    fn entry(&self, name: &str, expected: Option<&str>) -> Result<Entry> {
        ensure!(
            self.schema_version == 1,
            "unsupported manifest schema version"
        );
        let mut names = BTreeSet::new();
        for e in &self.adapters {
            ensure!(
                managed::NAMES.contains(&e.name.as_str()) && names.insert(&e.name),
                "unsupported or duplicate adapter name"
            );
            ensure!(
                managed::valid_version(&e.version),
                "invalid adapter version"
            );
            ensure!(
                e.protocol_version == vibedoc_protocol::PROTOCOL_VERSION,
                "unsupported adapter protocol version"
            );
            ensure!(managed::valid_digest(&e.sha256), "invalid SHA-256");
            ensure!(
                managed::relative_path(&e.executable),
                "executable must be a contained relative path"
            );
            let (runtime, minimum) = if e.name == "python" {
                ("python3", [3, 10, 0])
            } else {
                ("node", [22, 0, 0])
            };
            ensure!(
                e.runtime.name == runtime && parse_version(&e.runtime.min_version)? >= minimum,
                "unsupported runtime requirement"
            );
            ensure!(!e.archive.is_empty(), "empty archive location");
        }
        let entry = self
            .adapters
            .iter()
            .find(|e| e.name == name)
            .context("adapter missing from manifest")?;
        if let Some(expected) = expected {
            ensure!(
                entry.version == expected,
                "manifest version does not match requested version"
            );
        }
        Ok(entry.clone())
    }
}
fn parse_version(s: &str) -> Result<[u64; 3]> {
    ensure!(managed::valid_version(s), "invalid runtime version");
    let parts: Vec<u64> = s
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    Ok([parts[0], parts[1], parts[2]])
}

trait Transport {
    fn fetch(&self, url: &Url, output: &mut dyn Write, limit: u64) -> Result<()>;
}
struct Https(Client);
impl Https {
    fn builder() -> reqwest::blocking::ClientBuilder {
        Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(300))
            .redirect(Policy::custom(|attempt| {
                if attempt.url().scheme() != "https" {
                    attempt.error("redirect must use HTTPS")
                } else if attempt.previous().len() > 5 {
                    attempt.error("too many redirects")
                } else {
                    attempt.follow()
                }
            }))
    }
    fn new() -> Result<Self> {
        Ok(Self(Self::builder().build()?))
    }
}
impl Transport for Https {
    fn fetch(&self, url: &Url, output: &mut dyn Write, limit: u64) -> Result<()> {
        ensure!(url.scheme() == "https", "downloads require HTTPS");
        let response = self.0.get(url.clone()).send()?.error_for_status()?;
        if let Some(size) = response.content_length() {
            ensure!(size <= limit, "download exceeds size limit");
        }
        copy_limited(response, output, limit)
    }
}
fn copy_limited(reader: impl Read, output: &mut dyn Write, limit: u64) -> Result<()> {
    let copied = std::io::copy(&mut reader.take(limit + 1), output)?;
    ensure!(copied <= limit, "content exceeds size limit");
    Ok(())
}
#[derive(Debug)]
enum Source {
    Local(PathBuf),
    Remote(Url),
}
impl Source {
    fn manifest(value: &str) -> Result<Self> {
        if let Ok(url) = Url::parse(value) {
            ensure!(url.scheme() == "https", "manifest URL must use HTTPS");
            Ok(Self::Remote(url))
        } else {
            Ok(Self::Local(PathBuf::from(value).canonicalize()?))
        }
    }
    fn artifact(&self, location: &str) -> Result<Self> {
        if let Ok(url) = Url::parse(location) {
            ensure!(url.scheme() == "https", "artifact URL must use HTTPS");
            return Ok(Self::Remote(url));
        }
        match self {
            Self::Local(path) => Ok(Self::Local(path.parent().unwrap().join(location))),
            Self::Remote(url) => {
                ensure!(
                    !location.starts_with('/'),
                    "remote artifact must be a relative URL or HTTPS URL"
                );
                let url = url.join(location)?;
                ensure!(
                    url.scheme() == "https",
                    "remote manifest cannot reference local artifacts"
                );
                Ok(Self::Remote(url))
            }
        }
    }
    fn read(&self, transport: &impl Transport, output: &mut dyn Write, limit: u64) -> Result<()> {
        match self {
            Self::Local(p) => copy_limited(File::open(p)?, output, limit),
            Self::Remote(url) => transport.fetch(url, output, limit),
        }
    }
}

fn lock(root: &Path) -> Result<File> {
    fs::create_dir_all(root)?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".lock"))?;
    file.lock_exclusive()?;
    Ok(file)
}
fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    serde_json::to_writer_pretty(&mut tmp, value)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path)?;
    Ok(())
}
fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn runtime(entry: &Entry) -> Result<()> {
    let output = tempfile::tempfile()?;
    let mut child = Command::new(&entry.runtime.name)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "install {} {}+ on PATH first",
                entry.runtime.name, entry.runtime.min_version
            )
        })?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("runtime version check timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    ensure!(status.success(), "runtime version check failed");
    use std::io::{Seek, SeekFrom};
    let mut output = output;
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    copy_limited(output, &mut bytes, 4096)?;
    let text = String::from_utf8(bytes)?;
    let text = text
        .trim()
        .strip_prefix("Python ")
        .unwrap_or(text.trim())
        .trim_start_matches('v');
    ensure!(
        parse_version(text)? >= parse_version(&entry.runtime.min_version)?,
        "{} {}+ is required",
        entry.runtime.name,
        entry.runtime.min_version
    );
    Ok(())
}

fn extract(archive: &Path, directory: &Path) -> Result<()> {
    extract_with_limits(archive, directory, EXTRACT_LIMIT, ENTRY_LIMIT)
}

fn extract_with_limits(
    archive: &Path,
    directory: &Path,
    size_limit: u64,
    entry_limit: usize,
) -> Result<()> {
    // Bound decoding too: tar extension headers and skipped entries must not bypass
    // the file-data budget. Allow bounded header/padding overhead separately.
    let decoded_limit = size_limit + (entry_limit as u64 + 2) * 1024 + 64 * 1024;
    let decoder = GzDecoder::new(File::open(archive)?).take(decoded_limit + 1);
    let mut archive = tar::Archive::new(decoder);
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for (index, item) in archive.entries()?.enumerate() {
        ensure!(index < entry_limit, "archive has too many entries");
        let mut item = item?;
        let raw = item.path()?;
        let mut path = PathBuf::new();
        for c in raw.components() {
            match c {
                Component::CurDir => (),
                Component::Normal(c) => path.push(c),
                _ => bail!("unsafe archive path"),
            }
        }
        let kind = item.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "archive contains links or special files"
        );
        ensure!(names.insert(path.clone()), "duplicate archive path");
        total = total
            .checked_add(item.size())
            .context("archive size overflow")?;
        ensure!(total <= size_limit, "archive exceeds extracted size limit");
        if path.as_os_str().is_empty() {
            ensure!(kind.is_dir(), "empty archive file path");
            continue;
        }
        let destination = directory.join(path);
        if kind.is_dir() {
            fs::create_dir_all(destination)?;
        } else {
            fs::create_dir_all(destination.parent().unwrap())?;
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            let copied = std::io::copy(&mut item, &mut file)?;
            ensure!(copied == item.size(), "truncated archive entry");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if item.header().mode()? & 0o111 != 0 {
                    0o755
                } else {
                    0o644
                };
                fs::set_permissions(destination, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    // Consume the gzip trailer as well, so truncation/CRC failures cannot be hidden after tar EOF.
    let mut decoder = archive.into_inner();
    copy_limited(
        &mut decoder,
        &mut std::io::sink(),
        size_limit - total + 64 * 1024,
    )?;
    ensure!(decoder.limit() > 0, "archive exceeds decoded size limit");
    Ok(())
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct FileState {
    sha256: String,
    executable: bool,
}
fn inventory(directory: &Path) -> Result<BTreeMap<PathBuf, FileState>> {
    let mut files = BTreeMap::new();
    for item in walkdir::WalkDir::new(directory).follow_links(false) {
        let item = item?;
        ensure!(
            !item.file_type().is_symlink(),
            "installed adapter contains a symlink"
        );
        if item.file_type().is_dir() {
            continue;
        }
        ensure!(
            item.file_type().is_file(),
            "installed adapter contains a special file"
        );
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            item.metadata()?.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = true;
        files.insert(
            item.path().strip_prefix(directory)?.to_path_buf(),
            FileState {
                sha256: digest(item.path())?,
                executable,
            },
        );
    }
    Ok(files)
}
fn probe(directory: &Path, entry: &Entry) -> Result<()> {
    let executable = directory.join(&entry.executable);
    ensure!(executable.is_file(), "adapter executable is missing");
    let workspace = tempfile::tempdir()?;
    let client = AdapterClient::start(AdapterRunOptions {
        name: entry.name.clone(),
        command: executable,
        workspace_root: workspace.path().into(),
        timeout: Duration::from_secs(10),
    })?;
    ensure!(
        client.initialized.adapter.version == entry.version,
        "adapter version does not match manifest"
    );
    client.shutdown()?;
    Ok(())
}

fn install(
    root: &Path,
    name: &str,
    source: &str,
    expected: Option<&str>,
    transport: &impl Transport,
) -> Result<Record> {
    let _lock = lock(root)?;
    let source_location = Source::manifest(source)?;
    let mut manifest_bytes = Vec::new();
    source_location.read(transport, &mut manifest_bytes, MANIFEST_LIMIT)?;
    let entry = serde_json::from_slice::<Manifest>(&manifest_bytes)?.entry(name, expected)?;
    let artifact = source_location.artifact(&entry.archive)?;
    runtime(&entry)?;
    let base = root.join(name);
    fs::create_dir_all(base.join("records"))?;
    fs::create_dir_all(base.join("versions"))?;
    fs::create_dir_all(base.join("cache"))?;
    let record_path = base.join("records").join(format!("{}.json", entry.version));
    let inventory_path = base
        .join("records")
        .join(format!("{}.files.json", entry.version));
    if let Some(record) = managed::read_record(&record_path, name)? {
        ensure!(
            record.sha256 == entry.sha256,
            "adapter version already recorded with a different checksum"
        );
        ensure!(
            record.executable == entry.executable,
            "adapter version already recorded with a different executable"
        );
        let directory = base.join(&record.directory);
        let intact = fs::read(&inventory_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<BTreeMap<PathBuf, FileState>>(&b).ok())
            .zip(inventory(&directory).ok())
            .is_some_and(|(a, b)| !a.is_empty() && a == b);
        if intact {
            probe(&directory, &entry)?;
            atomic_json(&base.join("active.json"), &record)?;
            return Ok(record);
        }
    }
    let cached = base.join("cache").join(format!("{}.tar.gz", entry.sha256));
    if !cached.is_file() || digest(&cached)? != entry.sha256 {
        let mut download = tempfile::NamedTempFile::new_in(base.join("cache"))?;
        artifact.read(transport, &mut download, ARCHIVE_LIMIT)?;
        ensure!(
            digest(download.path())? == entry.sha256,
            "archive SHA-256 mismatch"
        );
        download.as_file().sync_all()?;
        download.persist(&cached)?;
    }
    let staging = tempfile::Builder::new()
        .prefix(&format!("{}-{}-", entry.version, entry.sha256))
        .tempdir_in(base.join("versions"))?;
    extract(&cached, staging.path())?;
    probe(staging.path(), &entry)?;
    let files = inventory(staging.path())?;
    let record = Record {
        schema_version: 1,
        name: name.into(),
        version: entry.version,
        sha256: entry.sha256,
        directory: staging.path().strip_prefix(&base)?.into(),
        executable: entry.executable,
        source: source.into(),
    };
    // The staged directory is immutable after this point. Old directories remain usable by running checks.
    let kept = staging.keep();
    let activation = (|| -> Result<()> {
        atomic_json(&inventory_path, &files)?;
        atomic_json(&record_path, &record)?;
        atomic_json(&base.join("active.json"), &record)?;
        Ok(())
    })();
    if activation.is_err() && !record_path.exists() {
        let _ = fs::remove_dir_all(kept);
    }
    activation?;
    Ok(record)
}
fn remove(root: &Path, name: &str) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let _lock = lock(root)?;
    let base = root.join(name);
    if base.exists() {
        let trash = tempfile::tempdir_in(root)?;
        fs::rename(&base, trash.path().join(name))?;
        trash.close()?;
    }
    Ok(())
}
fn list(root: &Path) -> Result<Vec<serde_json::Value>> {
    let mut items = Vec::new();
    for name in managed::NAMES {
        let active = managed::read_record(&root.join(name).join("active.json"), name)?;
        let active_path = managed::resolve(root, name)?;
        let mut versions = Vec::new();
        let records = root.join(name).join("records");
        if records.exists() {
            for item in fs::read_dir(records)? {
                let path = item?.path();
                if path.extension().is_some_and(|e| e == "json")
                    && !path.to_string_lossy().ends_with(".files.json")
                {
                    let record =
                        managed::read_record(&path, name)?.context("record disappeared")?;
                    versions.push(serde_json::json!({"version":record.version, "sha256":record.sha256, "active":active.as_ref().is_some_and(|a| a.directory == record.directory)}));
                }
            }
        }
        versions.sort_by_key(|v| v["version"].as_str().unwrap().to_owned());
        let fallback = managed::on_path(name);
        items.push(serde_json::json!({"name":name,"versions":versions,"activePath":active_path,"pathFallback":fallback,"shadowed":active.is_some() && fallback.is_some()}));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    struct NoNetwork;
    impl Transport for NoNetwork {
        fn fetch(&self, _: &Url, _: &mut dyn Write, _: u64) -> Result<()> {
            bail!("unexpected network")
        }
    }
    fn archive(path: &Path, members: &[(&str, &[u8], u8)]) {
        let gzip = GzEncoder::new(File::create(path).unwrap(), Compression::default());
        let mut tar = tar::Builder::new(gzip);
        for (name, content, kind) in members {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o6755);
            header.set_size(content.len() as u64);
            header.set_entry_type(tar::EntryType::new(*kind));
            // Raw names deliberately allow traversal fixtures that Builder rejects.
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar.append(&header, *content).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }
    fn script(version: &str) -> String {
        format!(
            r#"#!/bin/sh
read -r line
printf '%s\n' '{{"jsonrpc":"2.0","id":1,"result":{{"protocolVersion":1,"adapter":{{"name":"python","version":"{version}","runtime":"test"}},"capabilities":{{"languages":[],"extensions":[],"relationships":[]}}}}}}'
read -r line
printf '%s\n' '{{"jsonrpc":"2.0","id":2,"result":{{}}}}'
exit 0
"#
        )
    }
    fn fixture(directory: &Path, version: &str, body: &str) -> (PathBuf, Manifest) {
        let archive_path = directory.join(format!("{version}.tar.gz"));
        archive(&archive_path, &[("./adapter", body.as_bytes(), b'0')]);
        let manifest = Manifest {
            schema_version: 1,
            adapters: vec![Entry {
                name: "python".into(),
                version: version.into(),
                protocol_version: 1,
                runtime: Runtime {
                    name: "python3".into(),
                    min_version: "3.10.0".into(),
                },
                archive: archive_path.file_name().unwrap().to_str().unwrap().into(),
                sha256: digest(&archive_path).unwrap(),
                executable: "adapter".into(),
            }],
        };
        let path = directory.join("adapters.json");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        (path, manifest)
    }
    #[test]
    fn manifest_rejects_unsupported_and_unsafe_values() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, good) = fixture(tmp.path(), "1.0.0", &script("1.0.0"));
        assert!(good.entry("python", Some("1.0.0")).is_ok());
        assert!(good.entry("python", Some("2.0.0")).is_err());
        let mutations: Vec<fn(&mut Manifest)> = vec![
            |m| m.schema_version = 2,
            |m| m.adapters[0].name = "other".into(),
            |m| m.adapters.push(m.adapters[0].clone()),
            |m| m.adapters[0].version = "latest".into(),
            |m| m.adapters[0].version = "01.2.3".into(),
            |m| m.adapters[0].protocol_version = 2,
            |m| m.adapters[0].sha256 = "x".repeat(64),
            |m| m.adapters[0].executable = "../adapter".into(),
            |m| m.adapters[0].executable = "/adapter".into(),
            |m| m.adapters[0].runtime.name = "sh".into(),
            |m| m.adapters[0].runtime.min_version = "3.9.0".into(),
        ];
        for mutate in mutations {
            let mut bad = good.clone();
            mutate(&mut bad);
            assert!(bad.entry("python", None).is_err(), "{bad:?}");
        }
        assert!(serde_json::from_slice::<Manifest>(b"{}").is_err());
    }
    #[test]
    fn installation_is_idempotent_repairs_damage_and_preserves_previous_on_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("store");
        let (path, manifest) = fixture(tmp.path(), "1.0.0", &script("1.0.0"));
        let source = path.to_str().unwrap();
        let first = install(&root, "python", source, None, &NoNetwork).unwrap();
        let again = install(&root, "python", source, None, &NoNetwork).unwrap();
        assert_eq!(first.directory, again.directory);
        fs::write(
            root.join("python").join(&first.directory).join("adapter"),
            "damaged",
        )
        .unwrap();
        fs::write(
            root.join("python/cache")
                .join(format!("{}.tar.gz", first.sha256)),
            "corrupt cache",
        )
        .unwrap();
        let repaired = install(&root, "python", source, None, &NoNetwork).unwrap();
        assert_ne!(first.directory, repaired.directory);
        let active_before = fs::read(root.join("python/active.json")).unwrap();
        fixture(tmp.path(), "1.0.0", &script("different checksum"));
        assert!(
            install(&root, "python", source, None, &NoNetwork)
                .unwrap_err()
                .to_string()
                .contains("different checksum")
        );
        for body in [
            script("wrong version"),
            script("2.0.0").replace("\"name\":\"python\"", "\"name\":\"other\""),
            script("2.0.0").replace("protocolVersion\":1", "protocolVersion\":2"),
            "#!/bin/sh\necho not-json\n".into(),
            script("2.0.0").replace("exit 0", "exit 7"),
        ] {
            fixture(tmp.path(), "2.0.0", &body);
            assert!(install(&root, "python", source, None, &NoNetwork).is_err());
            assert_eq!(
                fs::read(root.join("python/active.json")).unwrap(),
                active_before
            );
        }
        let mut bad = manifest;
        bad.adapters[0].version = "3.0.0".into();
        bad.adapters[0].sha256 = "0".repeat(64);
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            install(&root, "python", source, None, &NoNetwork)
                .unwrap_err()
                .to_string()
                .contains("SHA-256 mismatch")
        );
        assert_eq!(
            fs::read(root.join("python/active.json")).unwrap(),
            active_before
        );
        assert!(managed::resolve(&root, "python").unwrap().is_some());
        fs::create_dir_all(root.join("typescript")).unwrap();
        remove(&root, "python").unwrap();
        remove(&root, "python").unwrap();
        assert!(root.join("typescript").is_dir());
        assert!(managed::resolve(&root, "python").unwrap().is_none());
    }
    #[test]
    fn upgrades_reactivates_old_versions_and_rolls_back_interrupted_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("store");
        let (path, first_manifest) = fixture(tmp.path(), "1.0.0", &script("1.0.0"));
        let source = path.to_str().unwrap();
        let first = install(&root, "python", source, None, &NoNetwork).unwrap();
        let (_, mut second_manifest) = fixture(tmp.path(), "2.0.0", &script("2.0.0"));
        let second = install(&root, "python", source, None, &NoNetwork).unwrap();
        assert_ne!(first.directory, second.directory);
        assert_eq!(
            list(&root).unwrap()[0]["versions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        fs::write(&path, serde_json::to_vec(&first_manifest).unwrap()).unwrap();
        assert_eq!(
            install(&root, "python", source, None, &NoNetwork)
                .unwrap()
                .directory,
            first.directory
        );
        let active = fs::read(root.join("python/active.json")).unwrap();
        second_manifest.adapters[0].version = "3.0.0".into();
        second_manifest.adapters[0].sha256 = "a".repeat(64);
        second_manifest.adapters[0].archive = "https://example.invalid/archive".into();
        fs::write(&path, serde_json::to_vec(&second_manifest).unwrap()).unwrap();
        struct Interrupted;
        impl Transport for Interrupted {
            fn fetch(&self, _: &Url, out: &mut dyn Write, _: u64) -> Result<()> {
                out.write_all(b"partial")?;
                bail!("interrupted")
            }
        }
        assert!(install(&root, "python", source, None, &Interrupted).is_err());
        assert_eq!(fs::read(root.join("python/active.json")).unwrap(), active);
        assert_eq!(fs::read_dir(root.join("python/cache")).unwrap().count(), 2);
    }

    #[test]
    fn rejects_unsafe_archives_and_strips_privileged_bits() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.tar.gz");
        for name in ["../escape", "/absolute", "a/../../escape"] {
            archive(&path, &[(name, b"data", b'0')]);
            let out = tempfile::tempdir().unwrap();
            assert!(extract(&path, out.path()).is_err());
        }
        for kind in *b"12346" {
            archive(&path, &[("bad", b"", kind)]);
            assert!(extract(&path, tempfile::tempdir().unwrap().path()).is_err());
        }
        archive(&path, &[("a", b"a", b'0'), ("./a", b"b", b'0')]);
        assert!(extract(&path, tempfile::tempdir().unwrap().path()).is_err());
        archive(&path, &[("./", b"", b'5'), ("./adapter", b"ok", b'0')]);
        let out = tempfile::tempdir().unwrap();
        extract(&path, out.path()).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(out.path().join("adapter"))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o755
        );
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 6);
        fs::write(&path, bytes).unwrap();
        assert!(extract(&path, tempfile::tempdir().unwrap().path()).is_err());
    }
    #[test]
    fn archive_size_and_entry_budgets_are_enforced() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("archive.tar.gz");
        archive(
            &archive_path,
            &[("a", b"12345", b'0'), ("b", b"67890", b'0')],
        );
        assert!(
            extract_with_limits(&archive_path, tempfile::tempdir().unwrap().path(), 9, 20)
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        assert!(
            extract_with_limits(
                &archive_path,
                tempfile::tempdir().unwrap().path(),
                EXTRACT_LIMIT,
                1
            )
            .unwrap_err()
            .to_string()
            .contains("too many entries")
        );
    }

    #[test]
    fn source_boundaries_limits_and_interrupted_downloads() {
        let remote = Source::manifest("https://example.invalid/releases/adapters.json").unwrap();
        for bad in [
            "file:///tmp/adapter",
            "http://example.invalid/archive",
            "/tmp/archive",
        ] {
            assert!(remote.artifact(bad).is_err());
        }
        assert!(
            matches!(remote.artifact("adapter.tar.gz").unwrap(), Source::Remote(u) if u.as_str() == "https://example.invalid/releases/adapter.tar.gz")
        );
        assert!(Source::manifest("http://example.invalid/manifest").is_err());
        assert!(copy_limited(&b"1234"[..], &mut Vec::new(), 3).is_err());
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("interrupted transfer"))
            }
        }
        assert!(copy_limited(Broken, &mut Vec::new(), 10).is_err());
        let tmp = tempfile::tempdir().unwrap();
        struct Interrupted;
        impl Transport for Interrupted {
            fn fetch(&self, _: &Url, output: &mut dyn Write, _: u64) -> Result<()> {
                output.write_all(b"partial")?;
                bail!("interrupted transfer")
            }
        }
        assert!(
            install(
                tmp.path(),
                "python",
                "https://example.invalid/adapters.json",
                None,
                &Interrupted
            )
            .is_err()
        );
        assert!(!tmp.path().join("python/active.json").exists());
    }
    #[test]
    fn missing_and_old_runtimes_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, mut m) = fixture(tmp.path(), "1.0.0", &script("1.0.0"));
        m.adapters[0].runtime.min_version = "999.0.0".into();
        assert!(runtime(&m.adapters[0]).is_err());
        m.adapters[0].runtime.name = "vibedoc-runtime-that-does-not-exist".into();
        assert!(runtime(&m.adapters[0]).is_err());
    }
    #[test]
    fn simultaneous_installs_serialize_and_share_an_immutable_installation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("store");
        let (source, _) = fixture(tmp.path(), "1.0.0", &script("1.0.0"));
        std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                install(&root, "python", source.to_str().unwrap(), None, &NoNetwork).unwrap()
            });
            let b = scope.spawn(|| {
                install(&root, "python", source.to_str().unwrap(), None, &NoNetwork).unwrap()
            });
            assert_eq!(a.join().unwrap().directory, b.join().unwrap().directory);
        });
    }
}

#[cfg(test)]
mod https_tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    #[test]
    fn local_tls_server_checks_trust_redirects_status_limits_and_truncation() {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert = certified.cert.der().clone();
        let key =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.into())
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let handle = thread::spawn(move || {
            let config = Arc::new(config);
            while !server_stop.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let connection = rustls::ServerConnection::new(config.clone()).unwrap();
                let mut stream = rustls::StreamOwned::new(connection, socket);
                let mut request = Vec::new();
                let mut byte = [0];
                while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                    if stream.read_exact(&mut byte).is_err() {
                        break;
                    }
                    request.push(byte[0]);
                }
                let text = String::from_utf8_lossy(&request);
                let path = text.split_whitespace().nth(1).unwrap_or("");
                let response = match path {
                    "/ok" => "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".into(),
                    "/redirect" => {
                        "HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\n\r\n".into()
                    }
                    "/loop" => {
                        "HTTP/1.1 302 Found\r\nLocation: /loop\r\nContent-Length: 0\r\n\r\n".into()
                    }
                    "/http" => format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/ok\r\nContent-Length: 0\r\n\r\n"
                    ),
                    "/large" => "HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n".into(),
                    "/short" => "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nok".into(),
                    _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".into(),
                };
                let response = response.replacen("\r\n\r\n", "\r\nConnection: close\r\n\r\n", 1);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                stream.conn.send_close_notify();
                let _ = stream.flush();
                let _ = stream.sock.shutdown(std::net::Shutdown::Write);
                // Drain peer close records so TCP does not reset an unread TLS socket.
                let _ = std::io::copy(&mut stream, &mut std::io::sink());
            }
        });
        // A guard stops the fixture even if an assertion fails.
        struct ServerGuard(Arc<AtomicBool>, Option<thread::JoinHandle<()>>);
        impl Drop for ServerGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
                self.1.take().unwrap().join().unwrap();
            }
        }
        let _guard = ServerGuard(stop, Some(handle));
        let url = |path: &str| Url::parse(&format!("https://localhost:{port}{path}")).unwrap();
        let untrusted = Https(Https::builder().no_proxy().build().unwrap());
        assert!(untrusted.fetch(&url("/ok"), &mut Vec::new(), 10).is_err());
        let trusted = Https(
            Https::builder()
                .no_proxy()
                .add_root_certificate(reqwest::Certificate::from_der(cert.as_ref()).unwrap())
                .build()
                .unwrap(),
        );
        for path in ["/ok", "/redirect"] {
            let mut bytes = Vec::new();
            trusted.fetch(&url(path), &mut bytes, 10).unwrap();
            assert_eq!(bytes, b"ok");
        }
        for path in ["/loop", "/http", "/large", "/short", "/missing"] {
            assert!(
                trusted.fetch(&url(path), &mut Vec::new(), 10).is_err(),
                "{path}"
            );
        }
    }
}
