use orbit_protocol::session::{self, ClientMessage, ServerMessage};
use std::{
    collections::VecDeque,
    fmt,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
};

/// Events delivered from the local-session worker to the native event loop.
#[derive(Clone, Debug, PartialEq)]
pub enum TransportEvent {
    Server(ServerMessage),
    InvalidInput(String),
    Lost(String),
}

/// A semantic event could not enter the bounded transport queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendError {
    Full,
    Closed,
}

impl fmt::Display for SendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter.write_str("Venus input queue is full"),
            Self::Closed => formatter.write_str("Venus input channel is closed"),
        }
    }
}

impl std::error::Error for SendError {}

/// Bounded typed input handle for one transient Orbit attachment.
pub struct Transport {
    messages: mpsc::SyncSender<ClientMessage>,
    events: Arc<EventQueue>,
}

impl Transport {
    /// Connect on a worker thread, queue decoded server messages, and wake the event loop.
    #[must_use]
    pub fn start(socket: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (messages, receiver) = mpsc::sync_channel(256);
        let events = Arc::new(EventQueue::default());
        let notify = notifier(Arc::clone(&events), wake);
        if let Err(error) = thread::Builder::new()
            .name("venus-orbit-reader".into())
            .spawn({
                let notify = Arc::clone(&notify);
                move || run(socket, receiver, notify)
            })
        {
            notify(TransportEvent::Lost(format!(
                "Cannot start the Orbit transport worker: {error}"
            )));
        }
        Self { messages, events }
    }

    /// Queue one canonical Orbit message without blocking the native event loop.
    pub fn send(&self, message: ClientMessage) -> Result<(), SendError> {
        self.messages
            .try_send(message)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => SendError::Full,
                mpsc::TrySendError::Disconnected(_) => SendError::Closed,
            })
    }

    /// Drain ordered server events, with consecutive complete frames reduced to the latest one.
    pub fn drain_events(&self) -> Vec<TransportEvent> {
        self.events.drain()
    }
}

#[derive(Default)]
struct EventQueue(Mutex<QueuedEvents>);

#[derive(Default)]
struct QueuedEvents {
    events: VecDeque<TransportEvent>,
    frames: usize,
    delivered_frame_high_water: Option<u64>,
    stopped: bool,
}

const EVENT_QUEUE_CAPACITY: usize = 256;
const FRAME_QUEUE_CAPACITY: usize = 2;

impl EventQueue {
    fn push(&self, event: TransportEvent) -> bool {
        let mut state = self.0.lock().expect("transport event queue lock poisoned");
        if state.stopped {
            return false;
        }
        let incoming_revision = frame_revision(&event);
        if let Some(revision) = incoming_revision {
            let mut replaced = None;
            for (index, queued) in state.events.iter().enumerate().rev() {
                match queued {
                    TransportEvent::Server(ServerMessage::Accepted) => {}
                    TransportEvent::Server(ServerMessage::Frame(frame)) => {
                        let previous_revision = state
                            .events
                            .iter()
                            .take(index)
                            .filter_map(frame_revision)
                            .chain(state.delivered_frame_high_water)
                            .max();
                        if revision > frame.revision
                            && previous_revision.is_none_or(|previous| frame.revision > previous)
                        {
                            replaced = Some(index);
                        }
                        break;
                    }
                    _ => break,
                }
            }
            if let Some(index) = replaced {
                state.events.remove(index);
                state.events.push_back(event);
                return false;
            }
        }
        let wake = state.events.is_empty();
        if state.events.len() == EVENT_QUEUE_CAPACITY
            || (incoming_revision.is_some() && state.frames == FRAME_QUEUE_CAPACITY)
        {
            state.events.clear();
            state.events.push_back(TransportEvent::Lost(
                "Orbit event queue exceeded its bounded capacity".into(),
            ));
            state.frames = 0;
            state.stopped = true;
            return wake;
        }
        state.stopped = matches!(&event, TransportEvent::Lost(_));
        state.frames += usize::from(incoming_revision.is_some());
        state.events.push_back(event);
        wake
    }

    fn drain(&self) -> Vec<TransportEvent> {
        let mut state = self.0.lock().expect("transport event queue lock poisoned");
        state.delivered_frame_high_water = state
            .events
            .iter()
            .filter_map(frame_revision)
            .chain(state.delivered_frame_high_water)
            .max();
        state.frames = 0;
        state.events.drain(..).collect()
    }
}

fn frame_revision(event: &TransportEvent) -> Option<u64> {
    match event {
        TransportEvent::Server(ServerMessage::Frame(frame)) => Some(frame.revision),
        _ => None,
    }
}

fn notifier(
    events: Arc<EventQueue>,
    wake: impl Fn() + Send + Sync + 'static,
) -> Arc<dyn Fn(TransportEvent) + Send + Sync> {
    Arc::new(move |event| {
        if events.push(event) {
            wake();
        }
    })
}

fn run(
    socket: PathBuf,
    receiver: mpsc::Receiver<ClientMessage>,
    notify: Arc<dyn Fn(TransportEvent) + Send + Sync>,
) {
    let mut stream = match UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(error) => {
            notify(TransportEvent::Lost(format!(
                "Cannot connect to Orbit at {}: {error}",
                socket.display()
            )));
            return;
        }
    };
    let writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(error) => {
            notify(TransportEvent::Lost(format!(
                "Cannot open the Orbit input channel: {error}"
            )));
            return;
        }
    };
    if let Err(error) = write_message(
        &mut stream,
        &ClientMessage::Hello {
            minimum_version: session::VERSION,
            maximum_version: session::VERSION,
        },
    ) {
        notify(TransportEvent::Lost(format!(
            "Cannot start the Orbit attachment: {error}"
        )));
        return;
    }

    let writer_notify = Arc::clone(&notify);
    if let Err(error) = thread::Builder::new()
        .name("venus-orbit-writer".into())
        .spawn(move || write_loop(writer, receiver, writer_notify))
    {
        let _ = stream.shutdown(Shutdown::Both);
        notify(TransportEvent::Lost(format!(
            "Cannot start the Orbit input worker: {error}"
        )));
        return;
    }

    loop {
        match read_message(&mut stream) {
            Ok(Some(message)) => notify(TransportEvent::Server(message)),
            Ok(None) => {
                notify(TransportEvent::Lost(
                    "Orbit closed the local session".into(),
                ));
                return;
            }
            Err(error) => {
                notify(TransportEvent::Lost(format!(
                    "Orbit sent an invalid local-session message: {error}"
                )));
                return;
            }
        }
    }
}

fn write_loop(
    mut stream: UnixStream,
    receiver: mpsc::Receiver<ClientMessage>,
    notify: Arc<dyn Fn(TransportEvent) + Send + Sync>,
) {
    while let Ok(message) = receiver.recv() {
        if let Err(error) = write_message(&mut stream, &message) {
            let event = if error.kind() == std::io::ErrorKind::InvalidInput {
                TransportEvent::InvalidInput(error.to_string())
            } else {
                TransportEvent::Lost(format!("Cannot send input to Orbit: {error}"))
            };
            notify(event);
            if error.kind() != std::io::ErrorKind::InvalidInput {
                let _ = stream.shutdown(Shutdown::Both);
                return;
            }
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn write_message(stream: &mut UnixStream, message: &ClientMessage) -> std::io::Result<()> {
    let encoded = session::encode_client_message(message)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    stream.write_all(&encoded)
}

fn read_message(stream: &mut UnixStream) -> Result<Option<ServerMessage>, ReadError> {
    let mut header = [0; session::HEADER_BYTES];
    if stream.read(&mut header[..1])? == 0 {
        return Ok(None);
    }
    stream.read_exact(&mut header[1..])?;
    let length = session::server_message_len(&header)?
        .expect("a complete ORBS header declares a message length");
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(&header);
    bytes.resize(length, 0);
    stream.read_exact(&mut bytes[session::HEADER_BYTES..])?;
    Ok(Some(session::decode_server_message(&bytes)?))
}

#[derive(Debug)]
enum ReadError {
    Io(std::io::Error),
    Protocol(session::Error),
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Protocol(error) => error.fmt(formatter),
        }
    }
}

impl From<std::io::Error> for ReadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<session::Error> for ReadError {
    fn from(value: session::Error) -> Self {
        Self::Protocol(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_protocol::{
        Capabilities, Colors, Cursor, CursorShape, Dimensions, Frame, Rgb, Screen,
        session::FocusEvent,
    };
    use std::{
        fs,
        os::unix::net::UnixListener,
        time::{Duration, SystemTime},
    };

    #[test]
    fn local_queue_failures_name_venus_as_the_owner() {
        assert_eq!(SendError::Full.to_string(), "Venus input queue is full");
        assert_eq!(
            SendError::Closed.to_string(),
            "Venus input channel is closed"
        );
    }

    #[test]
    fn first_terminal_loss_is_the_final_transport_event() {
        let events = Arc::new(EventQueue::default());
        let queued = Arc::clone(&events);
        let (wakes, receiver) = mpsc::channel();
        let notify = notifier(queued, move || wakes.send(()).unwrap());

        notify(TransportEvent::Lost("writer failed".into()));
        notify(TransportEvent::Lost("socket closed".into()));
        notify(TransportEvent::InvalidInput("late input error".into()));
        notify(TransportEvent::Server(ServerMessage::Accepted));

        receiver.recv().unwrap();
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            events.drain(),
            [TransportEvent::Lost("writer failed".into())]
        );
    }

    #[test]
    fn frame_reduction_preserves_message_and_revision_order() {
        let events = EventQueue::default();
        assert!(events.push(server_frame(1)));
        assert!(!events.push(TransportEvent::Server(ServerMessage::Accepted)));
        assert!(!events.push(server_frame(2)));
        assert!(!events.push(TransportEvent::InvalidInput("input".into())));
        assert!(!events.push(server_frame(3)));
        assert!(!events.push(server_frame(4)));

        assert_eq!(
            events.drain(),
            [
                TransportEvent::Server(ServerMessage::Accepted),
                server_frame(2),
                TransportEvent::InvalidInput("input".into()),
                server_frame(4),
            ]
        );

        assert!(events.push(server_frame(3)));
        assert!(!events.push(server_frame(5)));
        assert_eq!(events.drain(), [server_frame(3), server_frame(5)]);

        assert!(events.push(server_frame(6)));
        assert!(!events.push(server_frame(5)));
        assert_eq!(events.drain(), [server_frame(6), server_frame(5)]);
    }

    #[test]
    fn decoded_frame_queue_is_bounded() {
        let events = EventQueue::default();
        events.push(server_frame(1));
        events.push(TransportEvent::InvalidInput("first barrier".into()));
        events.push(server_frame(2));
        events.push(TransportEvent::InvalidInput("second barrier".into()));
        events.push(server_frame(3));

        assert_eq!(
            events.drain(),
            [TransportEvent::Lost(
                "Orbit event queue exceeded its bounded capacity".into()
            )]
        );
    }

    #[test]
    fn typed_attachment_is_duplex_and_client_drop_leaves_server_listener() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.path).unwrap();
        let path = socket.path.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            assert_eq!(
                read_client(&mut stream),
                ClientMessage::Hello {
                    minimum_version: session::VERSION,
                    maximum_version: session::VERSION
                }
            );
            stream
                .write_all(
                    &session::encode_server_message(&ServerMessage::Attached {
                        version: session::VERSION,
                    })
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(
                read_client(&mut stream),
                ClientMessage::Focus(FocusEvent::Gained)
            );
            let mut eof = [0];
            assert_eq!(stream.read(&mut eof).unwrap(), 0);

            let (mut reopened, _) = listener.accept().unwrap();
            reopened
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            read_client(&mut reopened)
        });

        let (wakes, receiver) = mpsc::channel();
        let transport = Transport::start(path.clone(), move || {
            let _ = wakes.send(());
        });
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            transport.drain_events(),
            [TransportEvent::Server(ServerMessage::Attached {
                version: session::VERSION
            })]
        );
        transport
            .send(ClientMessage::Focus(FocusEvent::Gained))
            .unwrap();
        drop(transport);

        let mut reopened = UnixStream::connect(path).unwrap();
        write_message(
            &mut reopened,
            &ClientMessage::Hello {
                minimum_version: session::VERSION,
                maximum_version: session::VERSION,
            },
        )
        .unwrap();
        assert!(matches!(
            server.join().unwrap(),
            ClientMessage::Hello { .. }
        ));
    }

    #[test]
    fn invalid_server_header_becomes_one_bounded_loss_event() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_client(&mut stream);
            stream.write_all(&[0; session::HEADER_BYTES]).unwrap();
        });
        let (wakes, receiver) = mpsc::channel();
        let transport = Transport::start(socket.path.clone(), move || {
            let _ = wakes.send(());
        });
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let event = transport.drain_events().pop().unwrap();
        assert!(matches!(
            event,
            TransportEvent::Lost(detail) if detail.contains("invalid local-session message")
        ));
        server.join().unwrap();
    }

    fn read_client(stream: &mut UnixStream) -> ClientMessage {
        let mut header = [0; session::HEADER_BYTES];
        stream.read_exact(&mut header).unwrap();
        let length = session::client_message_len(&header).unwrap().unwrap();
        let mut bytes = Vec::from(header);
        bytes.resize(length, 0);
        stream
            .read_exact(&mut bytes[session::HEADER_BYTES..])
            .unwrap();
        session::decode_client_message(&bytes).unwrap()
    }

    fn server_frame(revision: u64) -> TransportEvent {
        TransportEvent::Server(ServerMessage::Frame(Box::new(Frame {
            revision,
            dimensions: Dimensions { cols: 0, rows: 0 },
            screen: Screen::Primary,
            title: String::new(),
            working_directory: String::new(),
            capabilities: Capabilities {
                hyperlinks: false,
                kitty_graphics: false,
            },
            colors: Colors {
                background: Rgb::BLACK,
                foreground: Rgb::BLACK,
                cursor: None,
                palette: [Rgb::BLACK; orbit_protocol::PALETTE_LEN],
            },
            cursor: Cursor {
                visible: false,
                blinking: false,
                password_input: false,
                shape: CursorShape::Block,
                viewport: None,
            },
            rows: Vec::new(),
        })))
    }

    struct TestSocket {
        directory: PathBuf,
        path: PathBuf,
    }

    impl TestSocket {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("venus-transport-{}-{nonce}", std::process::id()));
            fs::create_dir(&directory).unwrap();
            let path = directory.join("orbit.sock");
            Self { directory, path }
        }
    }

    impl Drop for TestSocket {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_dir(&self.directory);
        }
    }
}
