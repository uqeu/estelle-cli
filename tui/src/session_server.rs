// Server-owned Estelle sessions. The transport shape follows jcode's MIT-licensed
// server/client architecture; the implementation is original to Estelle's API contract.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use codex_uds::UnixListener;
use codex_uds::UnixStream;
use estelle_client::Client;
use estelle_client::Repo;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;
use tokio::io::ReadHalf;
use tokio::io::WriteHalf;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::AnswerReply;

#[cfg(unix)]
const SESSION_SOCKET_MODE: u32 = 0o600;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientFrame {
    Attach {
        repo: String,
        root: PathBuf,
        session_id: String,
    },
    Switch {
        session_id: String,
    },
    Request {
        request: ClientRequest,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ClientRequest {
    Ask {
        id: u64,
        question: String,
        session_context: Option<String>,
    },
    Command {
        id: u64,
        name: String,
        argument: String,
        last_question: Option<String>,
        skill_thread: Option<Vec<(String, String)>>,
    },
    Sweep {
        id: u64,
    },
    Cancel {
        id: u64,
    },
    FileRead {
        path: PathBuf,
    },
    FileChanged {
        path: PathBuf,
        summary: Option<String>,
    },
    AcknowledgeFileShifts {
        through: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    Snapshot {
        session_id: String,
        sessions: Vec<SessionSummary>,
        turns: Vec<SessionTurn>,
        active: Option<ActiveTurn>,
        file_shifts: Vec<FileShiftNotice>,
        /// JSON text avoids serde's internally-tagged + nested-flatten ambiguity when command turns and
        /// a fleet coexist in one reconnect snapshot. The decoded value is still FleetSnapshot end to end.
        fleet: Option<String>,
    },
    Started {
        active: ActiveTurn,
    },
    Completed {
        turn: SessionTurn,
    },
    SweepProgress {
        id: u64,
        progress: crate::top_level::SweepProgress,
    },
    Fleet {
        fleet: estelle_client::FleetSnapshot,
    },
    Cancelled {
        id: u64,
    },
    Rejected {
        id: u64,
        message: String,
    },
    FileShift {
        notice: FileShiftNotice,
    },
    FileActivityRecorded {
        path: PathBuf,
    },
    FileActivityRejected {
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct FileShiftNotice {
    pub(crate) id: u64,
    pub(crate) path: PathBuf,
    pub(crate) changed_by: String,
    pub(crate) summary: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SessionSummary {
    pub(crate) id: String,
    pub(crate) active: bool,
    pub(crate) turn_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ActiveTurn {
    pub(crate) id: u64,
    pub(crate) input: SessionInput,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SessionTurn {
    pub(crate) id: u64,
    pub(crate) input: SessionInput,
    pub(crate) outcome: SessionOutcome,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SessionInput {
    Question { question: String },
    Command { name: String, argument: String },
    Sweep,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SessionOutcome {
    Answer {
        answer: WireAnswer,
    },
    Command {
        reply: Box<crate::RemoteCommandReply>,
    },
    Sweep {
        lines: Vec<String>,
    },
    Failure {
        lines: [String; 3],
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WireAnswer {
    pub(crate) text: String,
    pub(crate) grounded: Option<bool>,
    pub(crate) degraded: bool,
    pub(crate) sources: Vec<WireSource>,
    pub(crate) working_paths: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WireSource {
    pub(crate) file: String,
    pub(crate) line: Option<u64>,
    pub(crate) extra: Map<String, Value>,
}

impl From<AnswerReply> for WireAnswer {
    fn from(answer: AnswerReply) -> Self {
        Self {
            text: answer.text,
            grounded: answer.grounded,
            degraded: answer.degraded,
            sources: answer
                .sources
                .into_iter()
                .map(|source| WireSource {
                    file: source.file,
                    line: source.line,
                    extra: source.extra,
                })
                .collect(),
            working_paths: answer.working_paths,
        }
    }
}

impl From<WireAnswer> for AnswerReply {
    fn from(answer: WireAnswer) -> Self {
        Self {
            text: answer.text,
            grounded: answer.grounded,
            degraded: answer.degraded,
            sources: answer
                .sources
                .into_iter()
                .map(|source| estelle_client::Source {
                    file: source.file,
                    line: source.line,
                    extra: source.extra,
                })
                .collect(),
            working_paths: answer.working_paths,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SessionKey {
    repo: String,
    root: PathBuf,
    id: String,
}

struct ActiveWork {
    turn: ActiveTurn,
    cancel: CancellationToken,
}

struct SessionState {
    id: String,
    repo: Repo,
    root: PathBuf,
    turns: Vec<SessionTurn>,
    active: Option<ActiveWork>,
    read_paths: VecDeque<PathBuf>,
    file_shifts: Vec<FileShiftNotice>,
    next_file_shift_id: u64,
    fleet: Option<estelle_client::FleetSnapshot>,
    events: broadcast::Sender<ServerMessage>,
}

impl SessionState {
    fn new(id: String, repo: Repo, root: PathBuf) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            id,
            repo,
            root,
            turns: Vec::new(),
            active: None,
            read_paths: VecDeque::new(),
            file_shifts: Vec::new(),
            next_file_shift_id: 1,
            fleet: None,
            events,
        }
    }
}

type SharedSession = Arc<Mutex<SessionState>>;
type Sessions = Arc<Mutex<HashMap<SessionKey, SharedSession>>>;

pub(crate) struct SessionServer {
    listener: UnixListener,
    socket_guard: SocketGuard,
    _startup_lock: StartupLock,
    client: Client,
    sessions: Sessions,
}

impl SessionServer {
    pub(crate) async fn bind(socket_path: PathBuf, client: Client) -> io::Result<Self> {
        let startup_lock = acquire_startup_lock(&socket_path).await?;
        prepare_socket_path(&socket_path).await?;
        let listener = UnixListener::bind(&socket_path).await?;
        set_socket_permissions(&socket_path).await?;
        Ok(Self {
            listener,
            socket_guard: SocketGuard { socket_path },
            _startup_lock: startup_lock,
            client,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(crate) async fn run(mut self, shutdown: CancellationToken) -> io::Result<()> {
        loop {
            let stream = tokio::select! {
                _ = shutdown.cancelled() => break,
                stream = self.listener.accept() => stream?,
            };
            let client = self.client.clone();
            let sessions = self.sessions.clone();
            tokio::spawn(async move {
                let _ = handle_connection(stream, client, sessions).await;
            });
        }
        drop(self.socket_guard);
        Ok(())
    }
}

pub(crate) struct SessionConnection {
    reader: BufReader<ReadHalf<UnixStream>>,
    writer: BufWriter<WriteHalf<UnixStream>>,
}

#[derive(Clone)]
pub(crate) struct SessionHandle {
    frames: mpsc::UnboundedSender<ClientFrame>,
}

impl SessionHandle {
    #[cfg(test)]
    pub(crate) fn test_channel() -> (Self, mpsc::UnboundedReceiver<ClientFrame>) {
        let (frames, receiver) = mpsc::unbounded_channel();
        (Self { frames }, receiver)
    }

    pub(crate) fn send(&self, request: ClientRequest) -> io::Result<()> {
        self.frames
            .send(ClientFrame::Request { request })
            .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "Estelle session connection closed"))
    }

    pub(crate) fn switch(&self, session_id: String) -> io::Result<()> {
        self.frames
            .send(ClientFrame::Switch { session_id })
            .map_err(|_| io::Error::new(ErrorKind::BrokenPipe, "Estelle session connection closed"))
    }
}

impl SessionConnection {
    pub(crate) async fn connect_named(
        socket_path: &Path,
        repo: Repo,
        root: PathBuf,
        session_id: &str,
    ) -> io::Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        let (reader, writer) = tokio::io::split(stream);
        let mut connection = Self {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
        };
        connection
            .write_frame(&ClientFrame::Attach {
                repo: repo.as_str().to_string(),
                root,
                session_id: session_id.to_string(),
            })
            .await?;
        Ok(connection)
    }

    pub(crate) async fn send(&mut self, request: ClientRequest) -> io::Result<()> {
        self.write_frame(&ClientFrame::Request { request }).await
    }

    #[cfg(test)]
    pub(crate) async fn switch(&mut self, session_id: &str) -> io::Result<()> {
        self.write_frame(&ClientFrame::Switch {
            session_id: session_id.to_string(),
        })
        .await
    }

    pub(crate) async fn next(&mut self) -> io::Result<ServerMessage> {
        read_json_line(&mut self.reader).await
    }

    async fn write_frame(&mut self, frame: &ClientFrame) -> io::Result<()> {
        write_json_line(&mut self.writer, frame).await
    }

    pub(crate) fn start(
        self,
    ) -> (
        SessionHandle,
        mpsc::UnboundedReceiver<Result<ServerMessage, String>>,
    ) {
        let (frames_tx, mut frames_rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let Self {
            mut reader,
            mut writer,
        } = self;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    frame = frames_rx.recv() => {
                        let Some(frame) = frame else {
                            return;
                        };
                        if let Err(error) = write_json_line(&mut writer, &frame).await {
                            let _ = events_tx.send(Err(error.to_string()));
                            return;
                        }
                    }
                    message = read_json_line::<ServerMessage, _>(&mut reader) => {
                        match message {
                            Ok(message) => {
                                if events_tx.send(Ok(message)).is_err() {
                                    return;
                                }
                            }
                            Err(error) => {
                                let _ = events_tx.send(Err(error.to_string()));
                                return;
                            }
                        }
                    }
                }
            }
        });
        (SessionHandle { frames: frames_tx }, events_rx)
    }
}

pub(crate) async fn record_hook_file_read(
    socket_path: &Path,
    repo: Repo,
    root: PathBuf,
    session_id: &str,
    path: PathBuf,
) -> io::Result<Vec<FileShiftNotice>> {
    record_hook_file_activity(
        socket_path,
        repo,
        root,
        session_id,
        ClientRequest::FileRead { path },
    )
    .await
}

pub(crate) async fn record_hook_file_change(
    socket_path: &Path,
    repo: Repo,
    root: PathBuf,
    session_id: &str,
    path: PathBuf,
    summary: Option<String>,
) -> io::Result<Vec<FileShiftNotice>> {
    record_hook_file_activity(
        socket_path,
        repo,
        root,
        session_id,
        ClientRequest::FileChanged { path, summary },
    )
    .await
}

async fn record_hook_file_activity(
    socket_path: &Path,
    repo: Repo,
    root: PathBuf,
    session_id: &str,
    request: ClientRequest,
) -> io::Result<Vec<FileShiftNotice>> {
    let mut connection =
        SessionConnection::connect_named(socket_path, repo, root, session_id).await?;
    let snapshot = tokio::time::timeout(std::time::Duration::from_secs(1), connection.next())
        .await
        .map_err(|_| io::Error::new(ErrorKind::TimedOut, "session snapshot timed out"))??;
    let ServerMessage::Snapshot { file_shifts, .. } = snapshot else {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "session server did not send an initial snapshot",
        ));
    };
    if let Some(last) = file_shifts.last() {
        connection
            .send(ClientRequest::AcknowledgeFileShifts { through: last.id })
            .await?;
    }
    connection.send(request).await?;
    loop {
        let message = tokio::time::timeout(std::time::Duration::from_secs(1), connection.next())
            .await
            .map_err(|_| {
                io::Error::new(ErrorKind::TimedOut, "file activity receipt timed out")
            })??;
        match message {
            ServerMessage::FileActivityRecorded { .. } => return Ok(file_shifts),
            ServerMessage::FileActivityRejected { message } => {
                return Err(io::Error::new(ErrorKind::InvalidInput, message));
            }
            _ => {}
        }
    }
}

pub(crate) fn default_socket_path() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".estelle").join("session.sock"))
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "home directory is unavailable"))
}

struct StartupLock(std::fs::File);

async fn acquire_startup_lock(socket_path: &Path) -> io::Result<StartupLock> {
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "session socket has no parent directory",
        )
    })?;
    codex_uds::prepare_private_socket_directory(parent).await?;
    let lock_path = socket_path.with_extension("lock");
    tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => io::Error::new(
                ErrorKind::AddrInUse,
                format!(
                    "Estelle session server startup is already owned at {}",
                    lock_path.display()
                ),
            ),
            std::fs::TryLockError::Error(error) => error,
        })?;
        Ok(StartupLock(file))
    })
    .await
    .map_err(|error| io::Error::other(format!("session startup lock task failed: {error}")))?
}

impl Drop for StartupLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

async fn handle_connection(
    stream: UnixStream,
    client: Client,
    sessions: Sessions,
) -> io::Result<()> {
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let ClientFrame::Attach {
        repo,
        root,
        session_id,
    } = read_json_line(&mut reader).await?
    else {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "first session frame must attach",
        ));
    };
    let repo = Repo::new(&repo)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "invalid repository name"))?;
    validate_session_id(&session_id)?;
    let root = tokio::fs::canonicalize(&root).await.unwrap_or(root);
    let key = SessionKey {
        repo: repo.as_str().to_string(),
        root: root.clone(),
        id: session_id.clone(),
    };
    let repo_name = repo.as_str().to_string();
    let mut session =
        get_or_create_session(&sessions, key, session_id, repo.clone(), root.clone()).await;
    let mut events = {
        let state = session.lock().await;
        state.events.subscribe()
    };
    let snapshot = session_snapshot(&session, &sessions).await;
    write_json_line(&mut writer, &snapshot).await?;

    loop {
        tokio::select! {
            frame = read_json_line::<ClientFrame, _>(&mut reader) => {
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(()),
                    Err(error) => return Err(error),
                };
                match frame {
                    ClientFrame::Request { request } => {
                        handle_request(
                            request,
                            session.clone(),
                            sessions.clone(),
                            client.clone(),
                        )
                        .await;
                    }
                    ClientFrame::Switch { session_id } => {
                        validate_session_id(&session_id)?;
                        let key = SessionKey {
                            repo: repo_name.clone(),
                            root: root.clone(),
                            id: session_id.clone(),
                        };
                        session = get_or_create_session(
                            &sessions,
                            key,
                            session_id,
                            repo.clone(),
                            root.clone(),
                        )
                        .await;
                        events = {
                            let state = session.lock().await;
                            state.events.subscribe()
                        };
                        let snapshot = session_snapshot(&session, &sessions).await;
                        write_json_line(&mut writer, &snapshot).await?;
                    }
                    ClientFrame::Attach { .. } => {
                        return Err(io::Error::new(ErrorKind::InvalidData, "client attached twice"));
                    }
                }
            }
            message = events.recv() => {
                match message {
                    Ok(message) => write_json_line(&mut writer, &message).await?,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let snapshot = session_snapshot(&session, &sessions).await;
                        write_json_line(&mut writer, &snapshot).await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

async fn get_or_create_session(
    sessions: &Sessions,
    key: SessionKey,
    session_id: String,
    repo: Repo,
    root: PathBuf,
) -> SharedSession {
    let mut sessions = sessions.lock().await;
    sessions
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(SessionState::new(session_id, repo, root))))
        .clone()
}

fn validate_session_id(session_id: &str) -> io::Result<()> {
    let mut chars = session_id.chars();
    let first = chars.next();
    let valid = session_id.len() <= 48
        && first.is_some_and(|ch| ch.is_ascii_alphanumeric())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(io::Error::new(
            ErrorKind::InvalidInput,
            "session name must be 1-48 ASCII letters, digits, '.', '_' or '-' and start with a letter or digit",
        ))
    }
}

async fn session_snapshot(session: &SharedSession, sessions: &Sessions) -> ServerMessage {
    let (session_id, repo, root, turns, active, file_shifts, fleet) = {
        let state = session.lock().await;
        (
            state.id.clone(),
            state.repo.as_str().to_string(),
            state.root.clone(),
            {
                let mut turns = state.turns.clone();
                if let Some(fleet) = &state.fleet {
                    // The snapshot carries the one authoritative fleet at top level. Keeping the same
                    // typed fleet nested in an old generic CommandReply hits serde's flattened-envelope
                    // ambiguity on replay; retain an honest count in the transcript and no second copy.
                    for turn in &mut turns {
                        let is_orchestra = matches!(
                            &turn.input,
                            SessionInput::Command { name, .. } if name == "orchestra"
                        );
                        if is_orchestra && let SessionOutcome::Command { reply } = &mut turn.outcome
                        {
                            reply.reply.count = fleet.total;
                            reply.reply.fleet = None;
                        }
                    }
                }
                turns
            },
            state.active.as_ref().map(|work| work.turn.clone()),
            state.file_shifts.clone(),
            state.fleet.clone(),
        )
    };
    let matching = {
        let sessions = sessions.lock().await;
        sessions
            .iter()
            .filter(|(key, _)| key.repo == repo && key.root == root)
            .map(|(_, session)| session.clone())
            .collect::<Vec<_>>()
    };
    let mut summaries = Vec::with_capacity(matching.len());
    for session in matching {
        let state = session.lock().await;
        summaries.push(SessionSummary {
            id: state.id.clone(),
            active: state.active.is_some(),
            turn_count: state.turns.len(),
        });
    }
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    ServerMessage::Snapshot {
        session_id,
        sessions: summaries,
        turns,
        active,
        file_shifts,
        fleet: fleet.and_then(|fleet| serde_json::to_string(&fleet).ok()),
    }
}

async fn handle_request(
    request: ClientRequest,
    session: SharedSession,
    sessions: Sessions,
    client: Client,
) {
    match request {
        ClientRequest::Ask {
            id,
            question,
            session_context,
        } => {
            let input = SessionInput::Question {
                question: question.clone(),
            };
            let Some((repo, root, cancel, _events)) = begin_work(&session, id, input.clone()).await
            else {
                return;
            };
            tokio::spawn(async move {
                let result = crate::answer_question(
                    client,
                    repo,
                    root,
                    question.clone(),
                    session_context,
                    &cancel,
                )
                .await;
                let outcome = match result {
                    Ok(answer) => SessionOutcome::Answer {
                        answer: answer.into(),
                    },
                    Err(estelle_client::Error::Cancelled) => {
                        return;
                    }
                    Err(error) => SessionOutcome::Failure {
                        lines: crate::failure_lines(&error),
                    },
                };
                finish_work(session, id, input, outcome).await;
            });
        }
        ClientRequest::Command {
            id,
            name,
            argument,
            last_question,
            skill_thread,
        } => {
            let Some(static_name) = crate::commands::resolve_session_name(&name) else {
                reject(&session, id, format!("unknown session command /{name}")).await;
                return;
            };
            let input = SessionInput::Command {
                name: static_name.to_string(),
                argument: argument.clone(),
            };
            let Some((repo, root, cancel, _events)) = begin_work(&session, id, input.clone()).await
            else {
                return;
            };
            let follows_orchestra = static_name == "orchestra";
            let pending = crate::PendingCommand {
                name: static_name,
                argument,
                last_question,
                skill_thread,
            };
            tokio::spawn(async move {
                let poll_client = client.clone();
                let poll_repo = repo.clone();
                let result =
                    crate::execute_remote_command(client, repo, root, pending, &cancel).await;
                let result = if follows_orchestra {
                    match result {
                        Ok(reply) => {
                            follow_orchestra(
                                poll_client,
                                poll_repo,
                                session.clone(),
                                reply,
                                &cancel,
                            )
                            .await
                        }
                        error => error,
                    }
                } else {
                    result
                };
                let outcome = match result {
                    Ok(reply) => SessionOutcome::Command {
                        reply: Box::new(reply),
                    },
                    Err(crate::CommandFailure::Client(estelle_client::Error::Cancelled)) => return,
                    Err(crate::CommandFailure::Client(error)) => SessionOutcome::Failure {
                        lines: crate::failure_lines(&error),
                    },
                    Err(crate::CommandFailure::Local(lines)) => SessionOutcome::Failure { lines },
                };
                finish_work(session, id, input, outcome).await;
            });
        }
        ClientRequest::Sweep { id } => {
            let input = SessionInput::Sweep;
            let Some((repo, root, cancel, events)) = begin_work(&session, id, input.clone()).await
            else {
                return;
            };
            tokio::spawn(async move {
                let result = crate::top_level::sweep_with_progress(
                    &client,
                    &repo,
                    &root,
                    false,
                    &cancel,
                    |progress| {
                        let _ = events.send(ServerMessage::SweepProgress { id, progress });
                        Ok(())
                    },
                )
                .await;
                let outcome = match result {
                    Ok(lines) => SessionOutcome::Sweep { lines },
                    Err(crate::top_level::SweepFailure::Client(
                        estelle_client::Error::Cancelled,
                    )) => return,
                    Err(crate::top_level::SweepFailure::Client(error)) => SessionOutcome::Failure {
                        lines: crate::failure_lines(&error),
                    },
                    Err(crate::top_level::SweepFailure::Local(error)) => SessionOutcome::Failure {
                        lines: [
                            format!("Sweep stopped: {error}"),
                            "The repository was not reported as fully swept.".to_string(),
                            "Correct the local or account state, then retry /sweep.".to_string(),
                        ],
                    },
                };
                finish_work(session, id, input, outcome).await;
            });
        }
        ClientRequest::Cancel { id } => {
            let mut state = session.lock().await;
            let Some(active) = state.active.take() else {
                return;
            };
            if active.turn.id != id {
                state.active = Some(active);
                return;
            }
            active.cancel.cancel();
            let _ = state.events.send(ServerMessage::Cancelled { id });
        }
        ClientRequest::FileRead { path } => {
            record_file_read(&session, path).await;
        }
        ClientRequest::FileChanged { path, summary } => {
            record_file_change(&session, &sessions, path, summary).await;
        }
        ClientRequest::AcknowledgeFileShifts { through } => {
            let mut state = session.lock().await;
            state.file_shifts.retain(|notice| notice.id > through);
        }
    }
}

fn normalize_session_path(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

async fn record_file_read(session: &SharedSession, path: PathBuf) {
    let mut state = session.lock().await;
    let Some(path) = normalize_session_path(&state.root, &path) else {
        let _ = state.events.send(ServerMessage::FileActivityRejected {
            message: "file activity path must stay inside the session repository".to_string(),
        });
        return;
    };
    state.read_paths.retain(|seen| seen != &path);
    state.read_paths.push_back(path.clone());
    if state.read_paths.len() > 4096 {
        state.read_paths.pop_front();
    }
    let _ = state
        .events
        .send(ServerMessage::FileActivityRecorded { path });
}

async fn record_file_change(
    current: &SharedSession,
    sessions: &Sessions,
    path: PathBuf,
    summary: Option<String>,
) {
    let (changed_by, repo, root, events, path) = {
        let state = current.lock().await;
        let Some(path) = normalize_session_path(&state.root, &path) else {
            let _ = state.events.send(ServerMessage::FileActivityRejected {
                message: "file activity path must stay inside the session repository".to_string(),
            });
            return;
        };
        (
            state.id.clone(),
            state.repo.as_str().to_string(),
            state.root.clone(),
            state.events.clone(),
            path,
        )
    };
    let peers = {
        let sessions = sessions.lock().await;
        sessions
            .iter()
            .filter(|(key, _)| key.repo == repo && key.root == root && key.id != changed_by)
            .map(|(_, session)| session.clone())
            .collect::<Vec<_>>()
    };
    for peer in peers {
        let mut state = peer.lock().await;
        if !state.read_paths.contains(&path) {
            continue;
        }
        let notice = FileShiftNotice {
            id: state.next_file_shift_id,
            path: path.clone(),
            changed_by: changed_by.clone(),
            summary: summary.clone(),
        };
        state.next_file_shift_id = state.next_file_shift_id.saturating_add(1);
        state.file_shifts.push(notice.clone());
        if state.file_shifts.len() > 64 {
            state.file_shifts.remove(0);
        }
        let _ = state.events.send(ServerMessage::FileShift { notice });
    }
    let _ = events.send(ServerMessage::FileActivityRecorded { path });
}

async fn begin_work(
    session: &SharedSession,
    id: u64,
    input: SessionInput,
) -> Option<(
    Repo,
    PathBuf,
    CancellationToken,
    broadcast::Sender<ServerMessage>,
)> {
    let mut state = session.lock().await;
    if state.active.is_some() {
        let _ = state.events.send(ServerMessage::Rejected {
            id,
            message: "this session already has work in progress".to_string(),
        });
        return None;
    }
    if state.turns.iter().any(|turn| turn.id == id) {
        let _ = state.events.send(ServerMessage::Rejected {
            id,
            message: "this session request ID was already used".to_string(),
        });
        return None;
    }
    let turn = ActiveTurn { id, input };
    let cancel = CancellationToken::new();
    state.active = Some(ActiveWork {
        turn: turn.clone(),
        cancel: cancel.clone(),
    });
    let _ = state.events.send(ServerMessage::Started { active: turn });
    Some((
        state.repo.clone(),
        state.root.clone(),
        cancel,
        state.events.clone(),
    ))
}

async fn reject(session: &SharedSession, id: u64, message: String) {
    let state = session.lock().await;
    let _ = state.events.send(ServerMessage::Rejected { id, message });
}

async fn finish_work(
    session: SharedSession,
    id: u64,
    input: SessionInput,
    outcome: SessionOutcome,
) {
    let mut state = session.lock().await;
    if state.active.as_ref().is_none_or(|work| work.turn.id != id) {
        return;
    }
    state.active = None;
    let turn = SessionTurn { id, input, outcome };
    state.turns.push(turn.clone());
    let _ = state.events.send(ServerMessage::Completed { turn });
}

fn fleet_is_terminal(fleet: &estelle_client::FleetSnapshot) -> bool {
    matches!(
        fleet.state.as_str(),
        "completed"
            | "failed"
            | "timed_out"
            | "killed"
            | "lost"
            | "blocked"
            | "needs_input"
            | "cancelled"
    )
}

async fn publish_fleet(session: &SharedSession, fleet: estelle_client::FleetSnapshot) {
    let mut state = session.lock().await;
    if state
        .fleet
        .as_ref()
        .is_some_and(|current| current.id == fleet.id && current.revision >= fleet.revision)
    {
        return;
    }
    state.fleet = Some(fleet.clone());
    let _ = state.events.send(ServerMessage::Fleet { fleet });
}

async fn follow_orchestra(
    client: Client,
    repo: Repo,
    session: SharedSession,
    mut reply: crate::RemoteCommandReply,
    cancel: &CancellationToken,
) -> Result<crate::RemoteCommandReply, crate::CommandFailure> {
    let accepted = reply.reply.orchestra_accepted() == Some(true);
    let job_id = reply
        .reply
        .orchestra_job_id()
        .unwrap_or_default()
        .to_string();
    let Some(mut fleet) = reply.reply.fleet.clone() else {
        return Err(crate::CommandFailure::Local([
            "Orchestra did not return a fleet snapshot.".to_string(),
            "The server accepted no revisioned live state.".to_string(),
            "Nothing was inferred from request timing; retry when the contract is available."
                .to_string(),
        ]));
    };
    if !accepted || job_id.is_empty() {
        return Err(crate::CommandFailure::Local([
            "Orchestra did not return a durable acceptance receipt.".to_string(),
            "A fleet without accepted=true and job_id cannot be followed safely.".to_string(),
            "Nothing was reported as running; retry after the server contract is repaired."
                .to_string(),
        ]));
    }
    publish_fleet(&session, fleet.clone()).await;
    while !fleet_is_terminal(&fleet) {
        let query = estelle_client::OrchestraStatusQuery::new(&job_id, fleet.revision);
        let status = client
            .orchestra_status(&repo, &query, cancel)
            .await
            .map_err(crate::CommandFailure::Client)?;
        if status.fleet.id != fleet.id {
            return Err(crate::CommandFailure::Local([
                "Orchestra status changed fleet identity.".to_string(),
                format!("Expected {}, received {}.", fleet.id, status.fleet.id),
                "The session refused to merge two server fleets.".to_string(),
            ]));
        }
        if status.fleet.revision > fleet.revision {
            fleet = status.fleet;
            reply.reply.fleet = Some(fleet.clone());
            if let Some(todo) = status.todo {
                reply.reply.todo = Some(todo);
            }
            publish_fleet(&session, fleet.clone()).await;
        }
    }
    Ok(reply)
}

async fn read_json_line<T, R>(reader: &mut R) -> io::Result<T>
where
    T: for<'de> Deserialize<'de>,
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            "session connection closed",
        ));
    }
    serde_json::from_str(&line).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

async fn write_json_line<T, W>(writer: &mut W, value: &T) -> io::Result<()>
where
    T: Serialize,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut bytes =
        serde_json::to_vec(value).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await
}

async fn prepare_socket_path(socket_path: &Path) -> io::Result<()> {
    let parent = socket_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "session socket has no parent directory",
        )
    })?;
    codex_uds::prepare_private_socket_directory(parent).await?;
    match UnixStream::connect(socket_path).await {
        Ok(_) => Err(io::Error::new(
            ErrorKind::AddrInUse,
            format!(
                "Estelle session server is already running at {}",
                socket_path.display()
            ),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
            if codex_uds::is_stale_socket_path(socket_path).await? {
                tokio::fs::remove_file(socket_path).await
            } else {
                Err(io::Error::new(
                    ErrorKind::AlreadyExists,
                    format!(
                        "session socket path is not a socket: {}",
                        socket_path.display()
                    ),
                ))
            }
        }
        Err(_error) if !socket_path.exists() => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
async fn set_socket_permissions(socket_path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(
        socket_path,
        std::fs::Permissions::from_mode(SESSION_SOCKET_MODE),
    )
    .await
}

#[cfg(not(unix))]
async fn set_socket_permissions(_socket_path: &Path) -> io::Result<()> {
    Ok(())
}

struct SocketGuard {
    socket_path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.socket_path)
            && error.kind() != ErrorKind::NotFound
        {
            tracing::warn!(
                socket_path = %self.socket_path.display(),
                %error,
                "failed to remove Estelle session socket"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use estelle_client::ApiKey;
    use estelle_client::Client;
    use estelle_client::Repo;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_json;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::query_param;

    use super::ClientRequest;
    use super::ServerMessage;
    use super::SessionConnection;
    use super::SessionInput;
    use super::SessionOutcome;
    use super::SessionServer;
    use super::SessionState;
    use super::SessionTurn;
    use super::normalize_session_path;
    use super::publish_fleet;
    use super::session_snapshot;

    fn fleet(id: &str, revision: u64, state: &str) -> estelle_client::FleetSnapshot {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "batch": "one",
            "state": state,
            "revision": revision,
            "observed_at": 1000.0,
            "stale_after_s": 60,
            "completed": 0,
            "total": 1,
            "agents": []
        }))
        .expect("fleet")
    }

    #[tokio::test]
    async fn a_new_orchestra_fleet_replaces_the_previous_terminal_run() {
        let session = Arc::new(Mutex::new(SessionState::new(
            "main".to_string(),
            Repo::new("fatelabs/estelle").expect("repo"),
            PathBuf::from("/tmp/estelle"),
        )));
        session.lock().await.fleet = Some(fleet("old", 9, "completed"));

        publish_fleet(&session, fleet("new", 1, "created")).await;

        let current = session.lock().await.fleet.clone().expect("current fleet");
        assert_eq!(current.id, "new");
        assert_eq!(current.revision, 1);
    }

    #[tokio::test]
    async fn fleet_snapshot_rewrite_never_changes_an_unrelated_command_count() {
        let session = Arc::new(Mutex::new(SessionState::new(
            "main".to_string(),
            Repo::new("fatelabs/estelle").expect("repo"),
            PathBuf::from("/tmp/estelle"),
        )));
        {
            let mut state = session.lock().await;
            state.fleet = Some(fleet("job_123", 2, "completed"));
            let reply = |count, fleet| crate::RemoteCommandReply {
                reply: estelle_client::CommandReply {
                    count,
                    fleet,
                    ..Default::default()
                },
                inspected_files: Vec::new(),
            };
            state.turns = vec![
                SessionTurn {
                    id: 1,
                    input: SessionInput::Command {
                        name: "outcomes".to_string(),
                        argument: String::new(),
                    },
                    outcome: SessionOutcome::Command {
                        reply: Box::new(reply(Some(8), None)),
                    },
                },
                SessionTurn {
                    id: 2,
                    input: SessionInput::Command {
                        name: "orchestra".to_string(),
                        argument: "inspect auth".to_string(),
                    },
                    outcome: SessionOutcome::Command {
                        reply: Box::new(reply(None, Some(fleet("job_123", 2, "completed")))),
                    },
                },
            ];
        }
        let sessions = Arc::new(Mutex::new(HashMap::new()));

        let ServerMessage::Snapshot { turns, .. } = session_snapshot(&session, &sessions).await
        else {
            panic!("expected snapshot");
        };
        let SessionOutcome::Command { reply: outcomes } = &turns[0].outcome else {
            panic!("expected outcomes reply");
        };
        let SessionOutcome::Command { reply: orchestra } = &turns[1].outcome else {
            panic!("expected orchestra reply");
        };
        assert_eq!(outcomes.reply.count, Some(8));
        assert!(outcomes.reply.fleet.is_none());
        assert_eq!(orchestra.reply.count, Some(1));
        assert!(orchestra.reply.fleet.is_none());
    }

    #[tokio::test]
    async fn work_survives_client_disconnect_and_is_replayed_on_reconnect() {
        let api = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/deep-search"))
            .and(body_json(serde_json::json!({
                "repo": "fatelabs/estelle",
                "question": "where does charge fail?"
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({
                        "answer": "The retry loop has no ceiling.",
                        "grounded": true,
                        "sources": [{"file": "api/charge.ts", "line": 52}]
                    })),
            )
            .expect(1)
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/me"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({
                        "plan": "founder",
                        "configured": ["anthropic"]
                    })),
            )
            .expect(1)
            .mount(&api)
            .await;
        let client = Client::new(
            &format!("{}/", api.uri()),
            ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let runtime = tempfile::tempdir().expect("runtime directory");
        let root = tempfile::tempdir().expect("working tree");
        let socket = runtime.path().join("session.sock");
        let shutdown = CancellationToken::new();
        let server = SessionServer::bind(socket.clone(), client.clone())
            .await
            .expect("bind session server");
        let duplicate = SessionServer::bind(socket.clone(), client).await;
        assert!(
            matches!(duplicate, Err(error) if error.kind() == std::io::ErrorKind::AddrInUse),
            "a second server must not steal the live session socket"
        );
        let server_task = tokio::spawn(server.run(shutdown.clone()));

        let mut first = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "main",
        )
        .await
        .expect("first client");
        let _snapshot = first.next().await.expect("initial snapshot");
        first
            .send(ClientRequest::Ask {
                id: 41,
                question: "where does charge fail?".to_string(),
                session_context: None,
            })
            .await
            .expect("submit question");
        drop(first);

        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut second = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "main",
        )
        .await
        .expect("second client");

        let ServerMessage::Snapshot { turns, active, .. } =
            second.next().await.expect("reconnect snapshot")
        else {
            panic!("first reconnect event was not a snapshot");
        };
        assert!(active.is_none(), "completed work must not remain active");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, 41);
        assert!(matches!(
            &turns[0].outcome,
            SessionOutcome::Answer { answer }
                if answer.text == "The retry loop has no ceiling."
        ));

        second
            .send(ClientRequest::Command {
                id: 42,
                name: "me".to_string(),
                argument: String::new(),
                last_question: None,
                skill_thread: None,
            })
            .await
            .expect("submit command");
        drop(second);
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut third = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "main",
        )
        .await
        .expect("third client");
        let ServerMessage::Snapshot { turns, active, .. } =
            third.next().await.expect("command reconnect snapshot")
        else {
            panic!("first command reconnect event was not a snapshot");
        };
        assert!(active.is_none());
        assert_eq!(turns.len(), 2);
        assert!(matches!(
            &turns[1].outcome,
            SessionOutcome::Command { reply }
                if reply.reply.extra.get("plan").and_then(serde_json::Value::as_str)
                    == Some("founder")
        ));

        shutdown.cancel();
        server_task
            .await
            .expect("server task")
            .expect("server exit");
    }

    #[tokio::test]
    async fn orchestra_fleet_keeps_running_and_replays_after_terminal_disconnect() {
        let api = MockServer::start().await;
        let created = serde_json::json!({
            "id": "job_123", "batch": "1 admitted assignment", "models": ["provider/model-a"],
            "state": "created", "attempt": "first", "revision": 1, "observed_at": 1000.0,
            "stale_after_s": 60, "completed": 0, "total": 1, "agents": [{
                "index": 1, "status": "queued", "attempt": "first", "state_observed_at": 1000.0,
                "progress": null, "assignments": {"attempted": null, "completed": null, "lost": null}
            }]
        });
        let mut completed = created.clone();
        completed["state"] = serde_json::json!("completed");
        completed["revision"] = serde_json::json!(2);
        completed["completed"] = serde_json::json!(1);
        completed["agents"][0]["status"] = serde_json::json!("completed");
        Mock::given(method("POST"))
            .and(path("/orchestra/run"))
            .and(body_json(serde_json::json!({
                "repo": "fatelabs/estelle", "task": "inspect auth"
            })))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "accepted": true, "job_id": "job_123", "fleet": created
            })))
            .expect(1)
            .mount(&api)
            .await;
        Mock::given(method("GET"))
            .and(path("/orchestra/status"))
            .and(query_param("fleet_id", "job_123"))
            .and(query_param("after_revision", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({"fleet": completed})),
            )
            .expect(1)
            .mount(&api)
            .await;
        let client = Client::new(
            &format!("{}/", api.uri()),
            ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let runtime = tempfile::tempdir().expect("runtime directory");
        let root = tempfile::tempdir().expect("working tree");
        let socket = runtime.path().join("session.sock");
        let shutdown = CancellationToken::new();
        let server = SessionServer::bind(socket.clone(), client)
            .await
            .expect("bind session server");
        let server_task = tokio::spawn(server.run(shutdown.clone()));
        let mut first = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "main",
        )
        .await
        .expect("first client");
        let _ = first.next().await.expect("initial snapshot");
        first
            .send(ClientRequest::Command {
                id: 71,
                name: "orchestra".to_string(),
                argument: "inspect auth".to_string(),
                last_question: None,
                skill_thread: None,
            })
            .await
            .expect("start orchestra");
        drop(first);

        tokio::time::sleep(Duration::from_millis(250)).await;
        let mut second = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "main",
        )
        .await
        .expect("reconnect");
        let ServerMessage::Snapshot {
            fleet,
            turns,
            active,
            ..
        } = second.next().await.expect("reconnect snapshot")
        else {
            panic!("reconnect did not receive a snapshot");
        };
        assert!(active.is_none() && turns.len() == 1);
        let fleet: estelle_client::FleetSnapshot =
            serde_json::from_str(&fleet.expect("server-owned fleet replay"))
                .expect("typed fleet replay");
        assert_eq!(fleet.revision, 2);
        assert_eq!(fleet.state, "completed");

        shutdown.cancel();
        server_task
            .await
            .expect("server task")
            .expect("server exit");
    }

    #[tokio::test]
    async fn named_sessions_in_one_repository_are_independent_and_discoverable() {
        let api = MockServer::start().await;
        for (question, answer) in [
            ("inspect payments", "payments are isolated"),
            ("inspect retries", "retries are isolated"),
        ] {
            Mock::given(method("POST"))
                .and(path("/deep-search"))
                .and(body_json(serde_json::json!({
                    "repo": "fatelabs/estelle",
                    "question": question,
                })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(100))
                        .set_body_json(serde_json::json!({
                            "answer": answer,
                            "grounded": true,
                            "sources": [],
                        })),
                )
                .expect(1)
                .mount(&api)
                .await;
        }
        let client = Client::new(
            &format!("{}/", api.uri()),
            ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let runtime = tempfile::tempdir().expect("runtime directory");
        let root = tempfile::tempdir().expect("working tree");
        let socket = runtime.path().join("session.sock");
        let shutdown = CancellationToken::new();
        let server = SessionServer::bind(socket.clone(), client)
            .await
            .expect("bind session server");
        let server_task = tokio::spawn(server.run(shutdown.clone()));

        let mut payments = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "payments",
        )
        .await
        .expect("payments session");
        let ServerMessage::Snapshot {
            session_id,
            sessions,
            ..
        } = payments.next().await.expect("payments snapshot")
        else {
            panic!("payments did not receive a snapshot");
        };
        assert_eq!(session_id, "payments");
        assert_eq!(
            sessions
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["payments"]
        );
        payments
            .send(ClientRequest::Ask {
                id: 51,
                question: "inspect payments".to_string(),
                session_context: None,
            })
            .await
            .expect("submit payments work");

        let mut retries = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "retries",
        )
        .await
        .expect("retries session");
        let ServerMessage::Snapshot {
            session_id,
            sessions,
            ..
        } = retries.next().await.expect("retries snapshot")
        else {
            panic!("retries did not receive a snapshot");
        };
        assert_eq!(session_id, "retries");
        assert_eq!(
            sessions
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["payments", "retries"]
        );
        retries
            .send(ClientRequest::Ask {
                id: 52,
                question: "inspect retries".to_string(),
                session_context: None,
            })
            .await
            .expect("submit retries work while payments is active");

        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut payments_replay = SessionConnection::connect_named(
            &socket,
            Repo::new("fatelabs/estelle").expect("repo"),
            root.path().to_path_buf(),
            "payments",
        )
        .await
        .expect("payments replay");
        let ServerMessage::Snapshot { turns, .. } = payments_replay
            .next()
            .await
            .expect("payments replay snapshot")
        else {
            panic!("payments replay did not receive a snapshot");
        };
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, 51);
        assert!(matches!(
            &turns[0].outcome,
            SessionOutcome::Answer { answer } if answer.text == "payments are isolated"
        ));

        payments_replay
            .switch("retries")
            .await
            .expect("switch watched session");
        let ServerMessage::Snapshot {
            session_id, turns, ..
        } = payments_replay
            .next()
            .await
            .expect("retries switch snapshot")
        else {
            panic!("switch did not receive a snapshot");
        };
        assert_eq!(session_id, "retries");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].id, 52);
        assert!(matches!(
            &turns[0].outcome,
            SessionOutcome::Answer { answer } if answer.text == "retries are isolated"
        ));

        shutdown.cancel();
        server_task
            .await
            .expect("server task")
            .expect("server exit");
    }

    #[test]
    fn file_activity_paths_are_lexically_confined_to_the_session_root() {
        let root = Path::new("/work/repository");

        assert_eq!(
            normalize_session_path(root, Path::new("./src/lib.rs")),
            Some(PathBuf::from("src/lib.rs"))
        );
        assert_eq!(
            normalize_session_path(root, Path::new("/work/repository/src/lib.rs")),
            Some(PathBuf::from("src/lib.rs"))
        );
        assert_eq!(normalize_session_path(root, Path::new("../secret")), None);
        assert_eq!(
            normalize_session_path(root, Path::new("/work/other/secret")),
            None
        );
        assert_eq!(normalize_session_path(root, Path::new(".")), None);
    }

    #[tokio::test]
    async fn peer_change_is_replayed_to_a_detached_session_that_read_the_file() {
        let api = MockServer::start().await;
        let client = Client::new(
            &format!("{}/", api.uri()),
            ApiKey::new("test-key").expect("key"),
            Duration::from_secs(120),
        )
        .expect("client");
        let runtime = tempfile::tempdir().expect("runtime directory");
        let root = tempfile::tempdir().expect("working tree");
        let socket = runtime.path().join("session.sock");
        let shutdown = CancellationToken::new();
        let server = SessionServer::bind(socket.clone(), client)
            .await
            .expect("bind session server");
        let server_task = tokio::spawn(server.run(shutdown.clone()));
        let repo = Repo::new("fatelabs/estelle").expect("repo");

        let mut reader = SessionConnection::connect_named(
            &socket,
            repo.clone(),
            root.path().to_path_buf(),
            "reader",
        )
        .await
        .expect("reader session");
        let _ = reader.next().await.expect("reader snapshot");
        reader
            .send(ClientRequest::FileRead {
                path: PathBuf::from("src/lib.rs"),
            })
            .await
            .expect("record reader touch");
        assert!(matches!(
            reader.next().await.expect("reader touch receipt"),
            ServerMessage::FileActivityRecorded { path }
                if path == Path::new("src/lib.rs")
        ));
        drop(reader);

        let mut writer = SessionConnection::connect_named(
            &socket,
            repo.clone(),
            root.path().to_path_buf(),
            "writer",
        )
        .await
        .expect("writer session");
        let _ = writer.next().await.expect("writer snapshot");
        writer
            .send(ClientRequest::FileChanged {
                path: PathBuf::from("src/lib.rs"),
                summary: Some("edited lines 10-20".to_string()),
            })
            .await
            .expect("record writer change");
        assert!(matches!(
            writer.next().await.expect("writer change receipt"),
            ServerMessage::FileActivityRecorded { path }
                if path == Path::new("src/lib.rs")
        ));
        drop(writer);
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut replay = SessionConnection::connect_named(
            &socket,
            repo.clone(),
            root.path().to_path_buf(),
            "reader",
        )
        .await
        .expect("reader replay");
        let ServerMessage::Snapshot { file_shifts, .. } =
            replay.next().await.expect("reader replay snapshot")
        else {
            panic!("reader replay did not receive a snapshot");
        };
        assert_eq!(file_shifts.len(), 1);
        assert_eq!(file_shifts[0].path, PathBuf::from("src/lib.rs"));
        assert_eq!(file_shifts[0].changed_by, "writer");
        assert_eq!(
            file_shifts[0].summary.as_deref(),
            Some("edited lines 10-20")
        );

        let mut writer_replay =
            SessionConnection::connect_named(&socket, repo, root.path().to_path_buf(), "writer")
                .await
                .expect("writer replay");
        let ServerMessage::Snapshot { file_shifts, .. } =
            writer_replay.next().await.expect("writer replay snapshot")
        else {
            panic!("writer replay did not receive a snapshot");
        };
        assert!(
            file_shifts.is_empty(),
            "the writer must not receive its own file-shift warning"
        );

        shutdown.cancel();
        server_task
            .await
            .expect("server task")
            .expect("server exit");
    }
}
