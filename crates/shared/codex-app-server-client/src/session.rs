use std::time::Duration;

use crate::protocol::{
    ClientInfo, InitializeCapabilities, InitializeResponse, ThreadStartParams, ThreadStartResponse,
    TurnError, TurnStartParams, TurnStartResponse,
};
use crate::transport::{AsyncBufRead, AsyncWrite};
use crate::{
    ApprovalHandler, CodexAppServerClient, DenyAllApprovalHandler, Error, Event, EventCollector,
    EventStream, Result,
};

/// Options used when creating a high-level [`CodexSession`].
///
/// This wraps the generated `initialize` params plus process-spawn knobs so a
/// caller can connect, handshake, and start using the app-server in one call.
#[derive(Clone, Debug)]
pub struct SessionOptions {
    pub client_info: ClientInfo,
    pub capabilities: Option<InitializeCapabilities>,
    pub command: String,
    pub extra_args: Vec<String>,
    pub call_timeout: Option<Duration>,
}

impl SessionOptions {
    /// Creates default session options for a client name and version.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            client_info: ClientInfo::new(name, version),
            capabilities: None,
            command: "codex".to_owned(),
            extra_args: Vec::new(),
            call_timeout: None,
        }
    }

    /// Adds a human-readable title to the `initialize` client info.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.client_info = self.client_info.with_title(title);
        self
    }

    /// Uses a different binary than `codex` when spawning the app-server.
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = command.into();
        self
    }

    /// Appends one extra argument after `app-server` when spawning Codex.
    pub fn with_extra_arg(mut self, arg: impl Into<String>) -> Self {
        self.extra_args.push(arg.into());
        self
    }

    /// Appends a Codex `-c key=value` config override.
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_args.push("-c".to_owned());
        self.extra_args
            .push(format!("{}={}", key.into(), value.into()));
        self
    }

    /// Supplies explicit initialize capabilities.
    pub fn with_capabilities(mut self, capabilities: InitializeCapabilities) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Overrides the request/response timeout used by generated client calls.
    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = Some(timeout);
        self
    }
}

/// High-level app-server session that owns a client plus event stream.
///
/// `CodexSession` performs the `initialize` / `initialized` handshake before it
/// is returned, then offers convenience methods for common thread and text-turn
/// workflows while keeping the full low-level [`CodexAppServerClient`] exposed.
pub struct CodexSession {
    client: CodexAppServerClient,
    events: EventStream,
    initialize_response: InitializeResponse,
}

/// Result of a one-shot text turn.
///
/// The raw `thread` and `turn` responses are kept alongside the collected turn
/// events so callers can inspect IDs, model details, diffs, and errors without
/// re-querying the app-server.
#[derive(Clone, Debug)]
pub struct TextTurnResult {
    pub thread: ThreadStartResponse,
    pub turn: TurnStartResponse,
    pub events: EventCollector,
}

impl TextTurnResult {
    /// Returns the concatenated assistant text deltas observed for the turn.
    pub fn agent_message(&self) -> &str {
        self.events.agent_message()
    }

    /// Returns the latest unified diff observed for the turn, if any.
    pub fn latest_diff(&self) -> Option<&str> {
        self.events.latest_diff()
    }

    /// Returns turn-level errors observed while waiting for completion.
    pub fn errors(&self) -> &[TurnError] {
        self.events.errors()
    }
}

impl CodexSession {
    /// Spawns `codex app-server`, performs the handshake, and returns a session.
    pub async fn spawn(options: SessionOptions) -> Result<Self> {
        let (client, events) = CodexAppServerClient::spawn(&options.command, &options.extra_args)?;
        Self::handshake(client, events, options).await
    }

    /// Connects over caller-provided async streams and performs the handshake.
    ///
    /// This is primarily useful for tests and custom transports.
    pub async fn connect_streams<R, W>(
        reader: R,
        writer: W,
        options: SessionOptions,
    ) -> Result<Self>
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (client, events) = CodexAppServerClient::connect_streams(reader, writer);
        Self::handshake(client, events, options).await
    }

    /// Connects to an app-server Unix socket and performs the handshake.
    #[cfg(unix)]
    pub async fn connect_unix(
        path: impl AsRef<std::path::Path>,
        options: SessionOptions,
    ) -> Result<Self> {
        let (client, events) = CodexAppServerClient::connect_unix(path).await?;
        Self::handshake(client, events, options).await
    }

    async fn handshake(
        client: CodexAppServerClient,
        events: EventStream,
        options: SessionOptions,
    ) -> Result<Self> {
        let client = if let Some(timeout) = options.call_timeout {
            client.with_call_timeout(timeout)
        } else {
            client
        };
        let initialize_response = client
            .initialize(crate::protocol::InitializeParams {
                capabilities: options.capabilities,
                client_info: options.client_info,
            })
            .await?;
        client.send_initialized()?;
        Ok(Self {
            client,
            events,
            initialize_response,
        })
    }

    pub fn client(&self) -> &CodexAppServerClient {
        &self.client
    }

    /// Returns the initialize response captured during session setup.
    pub fn initialize_response(&self) -> &InitializeResponse {
        &self.initialize_response
    }

    /// Receives the next raw event without applying an approval policy.
    ///
    /// Most integrations should prefer [`Self::next_notification`],
    /// [`Self::collect_until_complete`], or the higher-level text helpers so
    /// server requests are answered promptly.
    pub async fn next_event(&mut self) -> Option<Event> {
        self.events.recv().await
    }

    /// Receives the next server notification, replying to server requests with
    /// the supplied [`ApprovalHandler`] while waiting.
    pub async fn next_notification<H>(
        &mut self,
        handler: &H,
    ) -> Option<crate::protocol::ServerNotification>
    where
        H: ApprovalHandler,
    {
        loop {
            match self.events.recv().await? {
                Event::Notification(notification) => return Some(notification),
                Event::Request(request) => {
                    let reply = handler.handle(&request.request).await;
                    let _ = reply.send(request);
                }
                Event::Closed => return None,
            }
        }
    }

    /// Starts a thread with fully-typed generated params.
    pub async fn start_thread(&self, params: ThreadStartParams) -> Result<ThreadStartResponse> {
        self.client.thread_start(params).await
    }

    /// Starts a thread using only a model override.
    ///
    /// This is a convenience wrapper around
    /// `start_thread(ThreadStartParams::new().model(model))`.
    pub async fn start_thread_with_model(
        &self,
        model: impl Into<String>,
    ) -> Result<ThreadStartResponse> {
        self.start_thread(ThreadStartParams::new().model(model))
            .await
    }

    /// Sends a turn with fully-typed generated params.
    pub async fn send_turn(&self, params: TurnStartParams) -> Result<TurnStartResponse> {
        self.client.turn_start(params).await
    }

    /// Sends one text input item to an existing thread.
    ///
    /// This is a convenience wrapper around
    /// `send_turn(TurnStartParams::text(thread_id, text))`.
    pub async fn send_text_turn(
        &self,
        thread_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<TurnStartResponse> {
        self.send_turn(TurnStartParams::text(thread_id, text)).await
    }

    /// Runs a one-shot text turn using a new default thread.
    ///
    /// Server requests are answered with [`DenyAllApprovalHandler`]. Use
    /// [`Self::run_text_turn_with_model_and_handler`] when the turn needs a
    /// specific model or a real approval/tool policy.
    pub async fn run_text_turn(&mut self, text: impl Into<String>) -> Result<TextTurnResult> {
        let handler = DenyAllApprovalHandler::default();
        self.run_text_turn_with_params_and_handler(ThreadStartParams::new(), text, &handler)
            .await
    }

    /// Runs a one-shot text turn with an explicit model and approval handler.
    ///
    /// The helper starts a new thread, sends the text turn, drains
    /// notifications until that turn completes, and returns the collected
    /// assistant text/diff/errors.
    pub async fn run_text_turn_with_model_and_handler<H>(
        &mut self,
        model: impl Into<String>,
        text: impl Into<String>,
        handler: &H,
    ) -> Result<TextTurnResult>
    where
        H: ApprovalHandler,
    {
        self.run_text_turn_with_params_and_handler(
            ThreadStartParams::new().model(model),
            text,
            handler,
        )
        .await
    }

    /// Runs a one-shot text turn with explicit `thread/start` params and an
    /// approval handler.
    ///
    /// This is the lowest-level text-turn convenience helper: callers can set
    /// the full typed [`ThreadStartParams`] instead of only the model, while
    /// still reusing the shared thread start, turn start, server-request
    /// handling, and event collection flow.
    pub async fn run_text_turn_with_params_and_handler<H>(
        &mut self,
        thread_params: ThreadStartParams,
        text: impl Into<String>,
        handler: &H,
    ) -> Result<TextTurnResult>
    where
        H: ApprovalHandler,
    {
        let thread = self.start_thread(thread_params).await?;
        let turn = self.send_text_turn(&thread.thread.id, text).await?;
        let events = self
            .wait_for_turn_completed(&thread.thread.id, &turn.turn.id, handler)
            .await?;
        Ok(TextTurnResult {
            thread,
            turn,
            events,
        })
    }

    /// Drains notifications until a specific turn reaches a terminal status.
    ///
    /// Any server request encountered while waiting is answered through
    /// `handler`. The returned [`EventCollector`] contains assistant text,
    /// latest diff, completion state, and turn errors observed for the named
    /// thread/turn pair.
    pub async fn wait_for_turn_completed<H>(
        &mut self,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        handler: &H,
    ) -> Result<EventCollector>
    where
        H: ApprovalHandler,
    {
        let mut collector = EventCollector::for_turn(thread_id, turn_id);
        self.collect_until_complete(&mut collector, handler).await?;
        Ok(collector)
    }

    /// Drains notifications for a turn and returns only the assistant text.
    pub async fn collect_agent_message<H>(
        &mut self,
        thread_id: impl Into<String>,
        turn_id: impl Into<String>,
        handler: &H,
    ) -> Result<String>
    where
        H: ApprovalHandler,
    {
        let collector = self
            .wait_for_turn_completed(thread_id, turn_id, handler)
            .await?;
        Ok(collector.agent_message().to_owned())
    }

    /// Drains notifications into an existing collector until it is complete.
    pub async fn collect_until_complete<H>(
        &mut self,
        collector: &mut EventCollector,
        handler: &H,
    ) -> Result<()>
    where
        H: ApprovalHandler,
    {
        while !collector.is_complete() {
            let Some(notification) = self.next_notification(handler).await else {
                return Err(Error::TransportClosed);
            };
            collector.observe_notification(&notification);
        }
        Ok(())
    }
}
