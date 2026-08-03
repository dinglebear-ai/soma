use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use serde::Deserialize;
use soma_fleet::{CommandExecutor, CommandOutput, CommandRequest, HostRecord};
use tokio_util::sync::CancellationToken;

use crate::{
    BuildContextFingerprint, BuildContextInspector, BuildContextPolicy, InfraError, InfraResult,
};

const OUTPUT_LIMIT: usize = 64 * 1024;
const SCRIPT: &str = r#"import hashlib,json,os,stat,sys
root,rel,max_files,max_bytes=sys.argv[1],sys.argv[2],int(sys.argv[3]),int(sys.argv[4])
def open_beneath(root,rel):
 fd=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
 try:
  for part in [p for p in root.split('/') if p]+[p for p in rel.split('/') if p and p!='.']:
   nxt=os.open(part,os.O_RDONLY|os.O_NOFOLLOW,dir_fd=fd)
   os.close(fd); fd=nxt
  return fd
 except Exception:
  os.close(fd); raise
h=hashlib.sha256(); counts=[0,0]
def feed(kind,path,meta):
 data=(kind+'\0'+path+'\0'+oct(meta.st_mode & 0o777)+'\0'+str(meta.st_size)+'\0').encode()
 h.update(len(data).to_bytes(8,'big')); h.update(data)
def walk(fd,path):
 meta=os.fstat(fd)
 if stat.S_ISDIR(meta.st_mode):
  feed('d',path,meta)
  with os.scandir(fd) as entries:
   for entry in sorted(entries,key=lambda e:e.name):
    child=os.open(entry.name,os.O_RDONLY|os.O_NOFOLLOW,dir_fd=fd)
    try: walk(child,entry.name if path=='.' else path+'/'+entry.name)
    finally: os.close(child)
 elif stat.S_ISREG(meta.st_mode):
  counts[0]+=1; counts[1]+=meta.st_size
  if counts[0]>max_files: raise RuntimeError('build context file limit exceeded')
  if counts[1]>max_bytes: raise RuntimeError('build context byte limit exceeded')
  feed('f',path,meta); os.lseek(fd,0,os.SEEK_SET)
  while True:
   chunk=os.read(fd,1024*1024)
   if not chunk: break
   h.update(chunk)
 else: raise RuntimeError('build context contains symlink or unsupported file type')
fd=open_beneath(root,rel)
try:
 if not stat.S_ISDIR(os.fstat(fd).st_mode): raise RuntimeError('build context is not a directory')
 walk(fd,'.')
 print(json.dumps({'sha256':h.hexdigest(),'file_count':counts[0],'byte_count':counts[1]}))
finally: os.close(fd)
"#;

/// Descriptor-confined build-context inspector backed by fleet command execution.
pub struct CommandBuildContextInspector<E> {
    executor: Arc<E>,
    policy: BuildContextPolicy,
}

impl<E> CommandBuildContextInspector<E> {
    /// Creates an inspector from an executor and explicit policy.
    #[must_use]
    pub fn new(executor: Arc<E>, policy: BuildContextPolicy) -> Self {
        Self { executor, policy }
    }

    /// Returns the active build-context policy.
    #[must_use]
    pub const fn policy(&self) -> &BuildContextPolicy {
        &self.policy
    }
}

#[async_trait]
impl<E> BuildContextInspector for CommandBuildContextInspector<E>
where
    E: CommandExecutor,
{
    async fn fingerprint(
        &self,
        host: &HostRecord,
        path: &Path,
        deadline: soma_ops::Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<BuildContextFingerprint> {
        let (root, relative) = self.policy.resolve(path)?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(SCRIPT.as_bytes());
        let bootstrap = format!("import base64;exec(base64.b64decode('{encoded}'))");
        let relative = if relative.as_os_str().is_empty() {
            ".".into()
        } else {
            relative.to_string_lossy().into_owned()
        };
        let request = CommandRequest::new(
            "python3",
            [
                "-c".to_owned(),
                bootstrap,
                root.to_string_lossy().into_owned(),
                relative,
                self.policy.max_files().to_string(),
                self.policy.max_bytes().to_string(),
            ],
            deadline,
        )
        .map_err(soma_fleet::FleetError::from)?
        .with_output_limits(OUTPUT_LIMIT, OUTPUT_LIMIT)
        .map_err(soma_fleet::FleetError::from)?;
        let wire = parse_output(
            host,
            self.executor.execute(host, &request, cancellation).await?,
        )?;
        let result = BuildContextFingerprint {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            sha256: wire.sha256,
            file_count: wire.file_count,
            byte_count: wire.byte_count,
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Deserialize)]
struct FingerprintWire {
    sha256: String,
    file_count: u32,
    byte_count: u64,
}

fn parse_output(host: &HostRecord, output: CommandOutput) -> InfraResult<FingerprintWire> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "build-context",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: String::from_utf8_lossy(output.stderr()).trim().to_owned(),
        });
    }
    if output.truncated() {
        return Err(InfraError::Parse {
            domain: "build-context",
            message: "fingerprint output was truncated".into(),
        });
    }
    serde_json::from_slice(output.stdout()).map_err(|error| InfraError::Parse {
        domain: "build-context",
        message: format!("invalid fingerprint payload: {error}"),
    })
}

#[cfg(test)]
#[path = "process_build_context_tests.rs"]
mod tests;
