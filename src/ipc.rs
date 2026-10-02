//! Local, bounded newline-delimited JSON. Socket failures never touch the agent.
use crate::model::Snapshot;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt, PermissionsExt},
    net::{UnixListener, UnixStream},
};
use std::{
    fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    thread,
};

pub const MAX_FRAME: usize = 8192;
#[derive(Debug)]
pub enum Event {
    Snapshot(u64, Snapshot),
    Disconnected(u64),
    Control(Control, std::sync::mpsc::SyncSender<String>),
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Quit,
    Readout,
    Tuck,
    Reveal,
    ResetPlacement,
    NextSession,
    ReloadSprites,
    Status,
    LoadSprites { path: PathBuf },
}
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum Message {
    Snapshot(Snapshot),
    Control { control: Control },
}

pub fn socket_path() -> PathBuf {
    std::env::var_os("OMP_PET_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(format!("/tmp/omp-pet-{}/events.sock", unsafe {
                libc::geteuid()
            }))
        })
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}
impl Server {
    pub fn bind(path: &Path) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("socket needs a parent directory"))?;
        match fs::create_dir(parent) {
            Ok(()) => fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let metadata = fs::symlink_metadata(parent)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::other(
                "socket directory must be owned by you with mode 0700",
            ));
        }
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::other(
                    "refusing to replace a non-owned socket path",
                ));
            }
            match UnixStream::connect(path) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "OMP Pet is already running",
                    ));
                }
                Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => fs::remove_file(path)?,
                Err(e) => return Err(e),
            }
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.to_owned(),
        })
    }

    pub fn start(self, deliver: impl Fn(Event) + Send + Sync + 'static) {
        let deliver = std::sync::Arc::new(deliver);
        thread::spawn(move || {
            let mut connection = 0;
            for stream in self.listener.incoming() {
                let Ok(stream) = stream else {
                    continue;
                };
                connection += 1;
                let id = connection;
                let deliver = deliver.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    loop {
                        match read_message(&mut reader) {
                            Ok(Some(Message::Snapshot(snapshot))) => {
                                deliver(Event::Snapshot(id, snapshot))
                            }
                            Ok(Some(Message::Control { control })) => {
                                let (send, receive) = std::sync::mpsc::sync_channel(1);
                                deliver(Event::Control(control, send));
                                if let Ok(response) =
                                    receive.recv_timeout(std::time::Duration::from_secs(30))
                                {
                                    use std::io::Write;
                                    let stream = reader.get_mut();
                                    let _ = stream
                                        .set_write_timeout(Some(std::time::Duration::from_secs(1)));
                                    let _ = stream.write_all(format!("{response}\n").as_bytes());
                                }
                            }
                            Ok(None) | Err(_) => break,
                        }
                    }
                    deliver(Event::Disconnected(id));
                });
            }
        });
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Message>> {
    // fill_buf consumes only one bounded frame; read_line would allocate without limit.
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::other("truncated frame"))
            };
        }
        let newline = buffer.iter().position(|b| *b == b'\n');
        let count = newline.map_or(buffer.len(), |i| i + 1);
        if bytes.len() + count > MAX_FRAME {
            return Err(io::Error::other("frame too large"));
        }
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if newline.is_some() {
            break;
        }
    }
    let message: Message = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if let Message::Snapshot(snapshot) = &message {
        snapshot.validate().map_err(io::Error::other)?;
    }
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_multiple_frames_and_rejects_truncation() {
        let frame = serde_json::to_string(&Snapshot::demo()).unwrap();
        let data = format!("{frame}\n{frame}\n");
        let mut reader = io::Cursor::new(data);
        assert!(read_message(&mut reader).unwrap().is_some());
        assert!(read_message(&mut reader).unwrap().is_some());
        assert!(read_message(&mut reader).unwrap().is_none());
        assert!(read_message(&mut io::Cursor::new(frame)).is_err());
    }
    #[test]
    fn bounds_unterminated_frames_and_rejects_unknown_protocol() {
        assert!(read_message(&mut io::Cursor::new(vec![b'x'; MAX_FRAME + 1])).is_err());
        let mut s = Snapshot::demo();
        s.version = 2;
        assert!(
            read_message(&mut io::Cursor::new(format!(
                "{}\n",
                serde_json::to_string(&s).unwrap()
            )))
            .is_err()
        );
    }
}
