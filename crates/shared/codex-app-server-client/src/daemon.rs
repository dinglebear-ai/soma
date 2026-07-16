use std::path::{Path, PathBuf};

use crate::{CodexSession, Result, SessionOptions};

#[derive(Clone, Debug)]
pub struct CodexDaemon {
    socket_path: PathBuf,
    extra_args: Vec<String>,
}

impl CodexDaemon {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            extra_args: Vec::new(),
        }
    }

    pub fn with_extra_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_args.push("-c".to_owned());
        self.extra_args
            .push(format!("{}={}", key.into(), value.into()));
        self
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn listen_url(&self) -> String {
        format!("unix://{}", self.socket_path.display())
    }

    pub fn app_server_args(&self) -> Vec<String> {
        let mut args = vec![
            "app-server".to_owned(),
            "--listen".to_owned(),
            self.listen_url(),
        ];
        args.extend(self.extra_args.clone());
        args
    }

    pub fn start_args(&self) -> Vec<String> {
        let mut args = vec!["app-server".to_owned(), "daemon".to_owned()];
        args.extend(self.extra_args.clone());
        args.push("start".to_owned());
        args
    }

    pub fn stop_args(&self) -> Vec<String> {
        vec![
            "app-server".to_owned(),
            "daemon".to_owned(),
            "stop".to_owned(),
        ]
    }

    #[cfg(unix)]
    pub async fn connect(&self, options: SessionOptions) -> Result<CodexSession> {
        CodexSession::connect_unix(&self.socket_path, options).await
    }
}
