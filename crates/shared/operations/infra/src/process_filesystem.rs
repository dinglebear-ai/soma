use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use tokio_util::sync::CancellationToken;

use crate::{
    FileFindRequest, FileKind, FileReadPolicy, FileSearch, FileTail, FileTailRequest,
    FilesystemQueryInspector, InfraError, InfraResult, PathRead, PathReadRequest,
};

const QUERY_OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const QUERY_CONTENT_LIMIT: usize = 11 * 1024 * 1024;
const QUERY_SCRIPT: &str = r#"import base64, fnmatch, json, os, stat, sys
mode, root, rel, display = sys.argv[1:5]
a, b, cap = sys.argv[5], sys.argv[6], int(sys.argv[7])
def open_beneath(root, rel):
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    for part in [p for p in root.split('/') if p] + [p for p in rel.split('/') if p]:
        nxt = os.open(part, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        os.close(fd); fd = nxt
    return fd
def walk(fd, shown, depth, max_depth, pattern, limit, visits, items):
    if visits[0] >= 10000 or len(items) >= limit: return True
    visits[0] += 1
    meta = os.fstat(fd)
    is_dir, is_file = stat.S_ISDIR(meta.st_mode), stat.S_ISREG(meta.st_mode)
    if pattern is None or (is_file and fnmatch.fnmatch(os.path.basename(shown), pattern)):
        items.append(shown)
        if len(items) >= limit: return True
    if not is_dir or depth >= max_depth: return False
    truncated = False
    try:
        with os.scandir(fd) as entries:
            for entry in sorted(entries, key=lambda e: e.name):
                if visits[0] >= 10000 or len(items) >= limit:
                    truncated = True; break
                try: child = os.open(entry.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
                except OSError: continue
                try:
                    if walk(child, shown.rstrip('/') + '/' + entry.name, depth + 1, max_depth, pattern, limit, visits, items): truncated = True
                finally: os.close(child)
    except OSError: pass
    return truncated
fd = open_beneath(root, rel)
try:
    meta = os.fstat(fd)
    if mode == 'read':
        if stat.S_ISDIR(meta.st_mode):
            names = sorted(os.listdir(fd)); limit = int(a)
            print(json.dumps({'kind':'directory','entries':names[:limit],'size':0,'truncated':len(names)>limit}))
        elif stat.S_ISREG(meta.st_mode):
            data = os.read(fd, cap + 1)
            print(json.dumps({'kind':'file','content_b64':base64.b64encode(data[:cap]).decode(),'entries':[],'size':meta.st_size,'truncated':len(data)>cap}))
        else: raise RuntimeError('unsupported file type')
    elif mode in ('tree','find'):
        items=[]; visits=[0]; limit=int(b); pattern=None if mode=='tree' else a
        truncated=walk(fd, display, 0, int(a) if mode=='tree' else int(sys.argv[8]), pattern, limit, visits, items)
        print(json.dumps({'kind':'directory','entries':items,'size':0,'truncated':truncated or visits[0]>=10000}))
    elif mode == 'tail':
        if not stat.S_ISREG(meta.st_mode): raise RuntimeError('not a regular file')
        start=max(0, meta.st_size-cap); os.lseek(fd,start,os.SEEK_SET); data=os.read(fd,cap)
        lines=data.decode('utf-8','replace').splitlines(); kept=lines[-int(a):]
        payload=('
'.join(kept)+ ('
' if kept else '')).encode()
        print(json.dumps({'kind':'file','content_b64':base64.b64encode(payload).decode(),'entries':[],'size':meta.st_size,'truncated':start>0,'line_count':len(kept)}))
finally: os.close(fd)
"#;

/// Descriptor-walking filesystem query driver backed by fleet command execution.
pub struct CommandFilesystemQueryInspector<E> {
    executor: Arc<E>,
    policy: FileReadPolicy,
}

impl<E> CommandFilesystemQueryInspector<E> {
    /// Creates a query inspector from an executor and explicit read policy.
    #[must_use]
    pub fn new(executor: Arc<E>, policy: FileReadPolicy) -> Self {
        Self { executor, policy }
    }
    /// Returns the active read policy.
    #[must_use]
    pub fn policy(&self) -> &FileReadPolicy {
        &self.policy
    }
}

#[async_trait]
impl<E> FilesystemQueryInspector for CommandFilesystemQueryInspector<E>
where
    E: CommandExecutor,
{
    async fn read_path(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &PathReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<PathRead> {
        let (root, relative) = self.policy.resolve(path)?;
        let mode = if request.tree() { "tree" } else { "read" };
        let a = if request.tree() {
            request.depth().to_string()
        } else {
            "200".into()
        };
        let b = if request.tree() { "500" } else { "0" };
        let wire = self
            .run(
                host,
                path,
                mode,
                &root,
                &relative,
                &a,
                b,
                self.policy.max_preview_bytes(),
                request.deadline(),
                cancellation,
            )
            .await?;
        Ok(PathRead {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            kind: wire.kind()?,
            content: decode(&wire.content_b64)?,
            entries: wire.entries,
            size_bytes: wire.size,
            truncated: wire.truncated,
        })
    }

    async fn find(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileFindRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileSearch> {
        let (root, relative) = self.policy.resolve(path)?;
        let wire = self
            .run_inner(
                host,
                path,
                "find",
                &root,
                &relative,
                request.pattern(),
                &request.limit().to_string(),
                self.policy.max_preview_bytes(),
                request.deadline(),
                cancellation,
                Some(request.depth()),
            )
            .await?;
        Ok(FileSearch {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            items: wire.entries.into_iter().map(PathBuf::from).collect(),
            truncated: wire.truncated,
        })
    }

    async fn tail(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileTailRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileTail> {
        let (root, relative) = self.policy.resolve(path)?;
        let wire = self
            .run(
                host,
                path,
                "tail",
                &root,
                &relative,
                &request.lines().to_string(),
                "0",
                self.policy.max_preview_bytes(),
                request.deadline(),
                cancellation,
            )
            .await?;
        let content = decode(&wire.content_b64)?;
        Ok(FileTail {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            line_count: wire.line_count.unwrap_or_else(|| {
                content
                    .split(|byte| *byte == b'\n')
                    .filter(|line| !line.is_empty())
                    .count()
            }),
            content,
            truncated: wire.truncated,
        })
    }
}

impl<E> CommandFilesystemQueryInspector<E>
where
    E: CommandExecutor,
{
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        host: &HostRecord,
        display: &Path,
        mode: &str,
        root: &Path,
        relative: &Path,
        a: &str,
        b: &str,
        cap: usize,
        deadline: soma_ops::Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<QueryWire> {
        self.run_inner(
            host,
            display,
            mode,
            root,
            relative,
            a,
            b,
            cap,
            deadline,
            cancellation,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_inner(
        &self,
        host: &HostRecord,
        display: &Path,
        mode: &str,
        root: &Path,
        relative: &Path,
        a: &str,
        b: &str,
        cap: usize,
        deadline: soma_ops::Timestamp,
        cancellation: &CancellationToken,
        depth: Option<u8>,
    ) -> InfraResult<QueryWire> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(QUERY_SCRIPT.as_bytes());
        let bootstrap = format!("import base64;exec(base64.b64decode('{encoded}'))");
        let relative = if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            relative.to_string_lossy().into_owned()
        };
        let mut args = vec![
            "-c".into(),
            bootstrap,
            mode.into(),
            root.to_string_lossy().into_owned(),
            relative,
            display.to_string_lossy().into_owned(),
            a.into(),
            b.into(),
            cap.min(QUERY_CONTENT_LIMIT).to_string(),
        ];
        if let Some(depth) = depth {
            args.push(depth.to_string());
        }
        let request = CommandRequest::new("python3", args, deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(QUERY_OUTPUT_LIMIT, 1024 * 1024)
            .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &request, cancellation).await?;
        parse_output(host, output)
    }
}

#[derive(Deserialize)]
struct QueryWire {
    kind: String,
    #[serde(default)]
    content_b64: String,
    #[serde(default)]
    entries: Vec<String>,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    truncated: bool,
    line_count: Option<usize>,
}

impl QueryWire {
    fn kind(&self) -> InfraResult<FileKind> {
        match self.kind.as_str() {
            "file" => Ok(FileKind::File),
            "directory" => Ok(FileKind::Directory),
            other => Err(InfraError::Parse {
                domain: "filesystem",
                message: format!("unknown query kind {other}"),
            }),
        }
    }
}

fn decode(value: &str) -> InfraResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| InfraError::Parse {
            domain: "filesystem",
            message: format!("invalid base64 content: {error}"),
        })
}

fn parse_output(host: &HostRecord, output: CommandOutput) -> InfraResult<QueryWire> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "filesystem",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
        });
    }
    if output.truncated() {
        return Err(InfraError::Parse {
            domain: "filesystem",
            message: "bounded query output was truncated".into(),
        });
    }
    serde_json::from_slice(output.stdout()).map_err(|error| InfraError::Parse {
        domain: "filesystem",
        message: format!("invalid query JSON: {error}"),
    })
}

#[cfg(test)]
#[path = "process_filesystem_tests.rs"]
mod tests;
