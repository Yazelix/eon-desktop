use orbit_protocol::session::{self, ClientMessage, ServerMessage};
use std::{
    fmt,
    io::{Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

/// Events delivered from the local-session worker to the native event loop.
#[derive(Clone, Debug, PartialEq)]
pub enum TransportEvent {
    Server(ServerMessage),
    InputRejected(String),
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
            Self::Full => formatter.write_str("Orbit input queue is full"),
            Self::Closed => formatter.write_str("Orbit input channel is closed"),
        }
    }
}

impl std::error::Error for SendError {}

enum Command {
    Message(ClientMessage),
    Stop,
}

/// Bounded typed input handle for one transient Orbit attachment.
pub struct Transport {
    commands: mpsc::SyncSender<Command>,
}

impl Transport {
    /// Connect on a worker thread and report decoded server messages through `notify`.
    #[must_use]
    pub fn start(socket: PathBuf, notify: impl Fn(TransportEvent) + Send + Sync + 'static) -> Self {
        let (commands, receiver) = mpsc::sync_channel(256);
        let notify = Arc::new(notify);
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
        Self { commands }
    }

    /// Queue one canonical Orbit message without blocking the native event loop.
    pub fn send(&self, message: ClientMessage) -> Result<(), SendError> {
        self.commands
            .try_send(Command::Message(message))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => SendError::Full,
                mpsc::TrySendError::Disconnected(_) => SendError::Closed,
            })
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Stop);
    }
}

fn run(
    socket: PathBuf,
    receiver: mpsc::Receiver<Command>,
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
    receiver: mpsc::Receiver<Command>,
    notify: Arc<dyn Fn(TransportEvent) + Send + Sync>,
) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::Message(message) => {
                if let Err(error) = write_message(&mut stream, &message) {
                    let event = if error.kind() == std::io::ErrorKind::InvalidInput {
                        TransportEvent::InputRejected(error.to_string())
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
            Command::Stop => {
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
    use orbit_protocol::session::FocusEvent;
    use std::{
        fs,
        os::unix::net::UnixListener,
        time::{Duration, SystemTime},
    };

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

        let (events, receiver) = mpsc::channel();
        let transport = Transport::start(path.clone(), move |event| {
            let _ = events.send(event);
        });
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            TransportEvent::Server(ServerMessage::Attached {
                version: session::VERSION
            })
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
        let (events, receiver) = mpsc::channel();
        let _transport = Transport::start(socket.path.clone(), move |event| {
            let _ = events.send(event);
        });
        let event = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
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
