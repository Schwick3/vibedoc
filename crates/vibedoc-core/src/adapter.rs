use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use vibedoc_protocol::{
    AnalyzeParams, AnalyzeResult, InitializeParams, InitializeResult, JSONRPC_VERSION,
    JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION,
};

#[derive(Debug, Clone)]
pub struct AdapterRunOptions {
    pub name: String,
    pub command: PathBuf,
    pub workspace_root: PathBuf,
    pub timeout: Duration,
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter `{name}` was not found on PATH; expected executable `{executable}`")]
    NotFound { name: String, executable: String },
    #[error("adapter override for `{name}` must be an absolute path: {path}")]
    OverrideNotAbsolute { name: String, path: PathBuf },
    #[error("adapter executable does not exist or is not executable: {0}")]
    NotExecutable(PathBuf),
    #[error("failed to start adapter `{name}` at {path}: {source}")]
    Spawn {
        name: String,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("adapter protocol I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("adapter timed out while waiting for `{method}` after {seconds} seconds")]
    Timeout { method: String, seconds: u64 },
    #[error("adapter closed stdout before replying to `{0}`")]
    Closed(String),
    #[error("adapter exited before replying to `{method}` with status {status}")]
    Exited { method: String, status: String },
    #[error("adapter returned malformed JSON: {message}; line: {line}")]
    Malformed { message: String, line: String },
    #[error("adapter returned a mismatched JSON-RPC response for `{method}`")]
    MismatchedResponse { method: String },
    #[error("adapter returned an error for `{method}` ({code}): {message}")]
    Rpc {
        method: String,
        code: i64,
        message: String,
    },
    #[error("adapter response for `{method}` has no result")]
    MissingResult { method: String },
    #[error("adapter result for `{method}` is invalid: {message}")]
    InvalidResult { method: String, message: String },
    #[error("adapter `{name}` uses protocol {actual}; Vibedoc requires protocol {expected}")]
    ProtocolMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },
    #[error("adapter `{requested}` identified itself as `{actual}` during initialization")]
    IdentityMismatch { requested: String, actual: String },
}

pub fn find_adapter(
    name: &str,
    overrides: &BTreeMap<String, PathBuf>,
) -> Result<PathBuf, AdapterError> {
    if let Some(path) = overrides.get(name) {
        if !path.is_absolute() {
            return Err(AdapterError::OverrideNotAbsolute {
                name: name.to_string(),
                path: path.clone(),
            });
        }
        if is_executable(path) {
            return Ok(path.clone());
        }
        return Err(AdapterError::NotExecutable(path.clone()));
    }

    let executable = format!("vibedoc-adapter-{name}");
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(&executable);
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(AdapterError::NotFound {
        name: name.to_string(),
        executable,
    })
}

pub struct AdapterClient {
    options: AdapterRunOptions,
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<String, std::io::Error>>,
    next_id: u64,
    pub initialized: InitializeResult,
}

impl AdapterClient {
    pub fn start(options: AdapterRunOptions) -> Result<Self, AdapterError> {
        let mut child = spawn_adapter(&options).map_err(|source| AdapterError::Spawn {
            name: options.name.clone(),
            path: options.command.clone(),
            source,
        })?;
        let input = child.stdin.take().expect("piped adapter stdin");
        let stdout = child.stdout.take().expect("piped adapter stdout");
        let (sender, output) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        let mut client = Self {
            options,
            child,
            input,
            output,
            next_id: 1,
            initialized: InitializeResult {
                protocol_version: 0,
                adapter: vibedoc_protocol::AdapterMetadata {
                    name: String::new(),
                    version: String::new(),
                    runtime: String::new(),
                },
                capabilities: vibedoc_protocol::AdapterCapabilities::default(),
            },
        };
        let initialized: InitializeResult = client.request(
            "initialize",
            InitializeParams {
                protocol_version: PROTOCOL_VERSION,
                core_version: env!("CARGO_PKG_VERSION").to_string(),
                workspace_root: normalize_path(&client.options.workspace_root),
            },
        )?;
        if initialized.protocol_version != PROTOCOL_VERSION {
            return Err(AdapterError::ProtocolMismatch {
                name: client.options.name.clone(),
                expected: PROTOCOL_VERSION,
                actual: initialized.protocol_version,
            });
        }
        if initialized.adapter.name != client.options.name {
            return Err(AdapterError::IdentityMismatch {
                requested: client.options.name.clone(),
                actual: initialized.adapter.name,
            });
        }
        client.initialized = initialized;
        Ok(client)
    }

    pub fn analyze(&mut self, params: AnalyzeParams) -> Result<AnalyzeResult, AdapterError> {
        self.request("analyze", params)
    }

    pub fn shutdown(mut self) -> Result<(), AdapterError> {
        let _: serde_json::Value = self.request("shutdown", serde_json::json!({}))?;
        let _ = self.child.wait();
        Ok(())
    }

    fn request<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: P,
    ) -> Result<R, AdapterError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = JsonRpcRequest::new(id, method, params);
        serde_json::to_writer(&mut self.input, &request)
            .map_err(|error| AdapterError::Io(std::io::Error::other(error)))?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;

        let line = match self.output.recv_timeout(self.options.timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => return Err(AdapterError::Io(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(AdapterError::Timeout {
                    method: method.to_string(),
                    seconds: self.options.timeout.as_secs(),
                });
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Ok(Some(status)) = self.child.try_wait() {
                    return Err(AdapterError::Exited {
                        method: method.to_string(),
                        status: status.to_string(),
                    });
                }
                return Err(AdapterError::Closed(method.to_string()));
            }
        };
        let response: JsonRpcResponse =
            serde_json::from_str(&line).map_err(|error| AdapterError::Malformed {
                message: error.to_string(),
                line: line.clone(),
            })?;
        if response.jsonrpc != JSONRPC_VERSION || response.id != id {
            return Err(AdapterError::MismatchedResponse {
                method: method.to_string(),
            });
        }
        if let Some(error) = response.error {
            return Err(AdapterError::Rpc {
                method: method.to_string(),
                code: error.code,
                message: error.message,
            });
        }
        let result = response.result.ok_or_else(|| AdapterError::MissingResult {
            method: method.to_string(),
        })?;
        serde_json::from_value(result).map_err(|error| AdapterError::InvalidResult {
            method: method.to_string(),
            message: error.to_string(),
        })
    }
}

fn spawn_adapter(options: &AdapterRunOptions) -> Result<Child, std::io::Error> {
    const RETRY_DELAYS: [Duration; 4] = [
        Duration::from_millis(10),
        Duration::from_millis(20),
        Duration::from_millis(40),
        Duration::from_millis(80),
    ];

    for delay in RETRY_DELAYS {
        match adapter_command(options).spawn() {
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                thread::sleep(delay);
            }
            result => return result,
        }
    }
    adapter_command(options).spawn()
}

fn adapter_command(options: &AdapterRunOptions) -> Command {
    let mut command = Command::new(&options.command);
    command
        .arg("--stdio")
        .current_dir(&options.workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    command
}

impl Drop for AdapterClient {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn script(contents: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("adapter");
        fs::write(&path, contents).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        (directory, path)
    }

    fn options(command: PathBuf, timeout: Duration) -> AdapterRunOptions {
        AdapterRunOptions {
            name: "test".into(),
            command,
            workspace_root: std::env::current_dir().unwrap(),
            timeout,
        }
    }

    #[test]
    fn explicit_override_must_be_absolute() {
        let mut overrides = BTreeMap::new();
        overrides.insert("typescript".to_string(), PathBuf::from("adapter"));
        assert!(matches!(
            find_adapter("typescript", &overrides),
            Err(AdapterError::OverrideNotAbsolute { .. })
        ));
    }

    #[test]
    fn negotiates_analyzes_and_shuts_down() {
        let (_directory, command) = script(
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *initialize*) printf '%s\n' 'adapter log' >&2; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"adapter":{"name":"test","version":"0.1.0","runtime":"sh"},"capabilities":{"languages":[],"extensions":[],"relationships":[]}}}' ;;
    *analyze*) printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"graph":{"files":[],"symbols":[],"relationships":[]},"diagnostics":[]}}' ;;
    *shutdown*) printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{}}'; exit 0 ;;
  esac
done
"#,
        );
        let mut client = AdapterClient::start(options(command, Duration::from_secs(5))).unwrap();
        let analyzed = client.analyze(AnalyzeParams::default()).unwrap();
        assert!(analyzed.graph.symbols.is_empty());
        client.shutdown().unwrap();
    }

    #[test]
    fn rejects_incompatible_protocol() {
        let (_directory, command) = script(
            r#"#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":2,"adapter":{"name":"test","version":"0.1.0","runtime":"sh"},"capabilities":{"languages":[],"extensions":[],"relationships":[]}}}'
"#,
        );
        assert!(matches!(
            AdapterClient::start(options(command, Duration::from_secs(5))),
            Err(AdapterError::ProtocolMismatch { .. })
        ));
    }

    #[test]
    fn rejects_mismatched_adapter_identity() {
        let (_directory, command) = script(
            r#"#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"adapter":{"name":"other","version":"0.1.0","runtime":"sh"},"capabilities":{"languages":[],"extensions":[],"relationships":[]}}}'
read -r line
"#,
        );
        match AdapterClient::start(options(command, Duration::from_secs(5))) {
            Err(AdapterError::IdentityMismatch { .. }) => {}
            Err(error) => panic!("expected an identity mismatch, received {error:?}"),
            Ok(_) => panic!("expected an identity mismatch, adapter initialized"),
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn retries_a_temporarily_busy_adapter_executable() {
        let directory = tempfile::tempdir().unwrap();
        let command = directory.path().join("adapter");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&command)
            .unwrap();
        file.write_all(
            br#"#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"adapter":{"name":"other","version":"0.1.1","runtime":"sh"},"capabilities":{"languages":[],"extensions":[],"relationships":[]}}}'
read -r line
"#,
        )
        .unwrap();
        file.sync_all().unwrap();
        let mut permissions = fs::metadata(&command).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&command, permissions).unwrap();

        let release_file = thread::spawn(move || {
            thread::sleep(Duration::from_millis(25));
            drop(file);
        });
        let result = AdapterClient::start(options(command, Duration::from_secs(5)));
        release_file.join().unwrap();

        assert!(matches!(result, Err(AdapterError::IdentityMismatch { .. })));
    }

    #[test]
    fn reports_malformed_and_mismatched_output() {
        let (_directory, command) = script("#!/bin/sh\nread -r line\nprintf 'not-json\\n'\n");
        assert!(matches!(
            AdapterClient::start(options(command, Duration::from_secs(5))),
            Err(AdapterError::Malformed { .. })
        ));

        let (_directory, command) = script(
            "#!/bin/sh\nread -r line\nprintf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":99,\"result\":{}}'\n",
        );
        assert!(matches!(
            AdapterClient::start(options(command, Duration::from_secs(5))),
            Err(AdapterError::MismatchedResponse { .. })
        ));
    }

    #[test]
    fn reports_timeout_and_nonzero_exit() {
        let (_directory, command) = script("#!/bin/sh\nread -r line\nsleep 2\n");
        assert!(matches!(
            AdapterClient::start(options(command, Duration::from_millis(10))),
            Err(AdapterError::Timeout { .. })
        ));

        let (_directory, command) = script(
            r#"#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"adapter":{"name":"test","version":"0.1.0","runtime":"sh"},"capabilities":{"languages":[],"extensions":[],"relationships":[]}}}'
read -r line
exit 7
"#,
        );
        let mut client = AdapterClient::start(options(command, Duration::from_secs(5))).unwrap();
        assert!(matches!(
            client.analyze(AnalyzeParams::default()),
            Err(AdapterError::Exited { .. }) | Err(AdapterError::Closed(_))
        ));
    }
}
