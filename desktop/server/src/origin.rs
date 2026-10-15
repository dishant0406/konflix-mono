//! Where one video's bytes can be read from, and how to read them from any offset.
//!
//! Direct play, the HLS feeder and the probe all want the same thing: a reader that starts at a
//! byte. A torrent, a debrid link and a file on disk each give one, so the rest of the server
//! never needs to know which it has.

use crate::sources::debrid::Debrid;
use akidev_core::torrent::{Engine, Reader};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncSeekExt, ReadBuf};

pub type BoxRead = Box<dyn AsyncRead + Send + Unpin>;

/// An open reader. For a torrent it also keeps the torrent pinned, so the engine does not drop
/// it while someone is reading.
pub struct Opened {
    reader: BoxRead,
    _pin: Option<Reader>,
}

impl AsyncRead for Opened {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

pub enum Origin {
    Torrent {
        engine: Arc<Engine>,
        info_hash: String,
        file_index: usize,
        length: u64,
        name: String,
    },
    Debrid {
        debrid: Arc<Debrid>,
        torrent_id: i64,
        file_id: i64,
        length: u64,
        name: String,
    },
    /// A file on this machine, which is how the tests stand in for a swarm.
    #[allow(dead_code)]
    File {
        path: PathBuf,
        length: u64,
        name: String,
    },
}

impl Origin {
    pub fn name(&self) -> &str {
        match self {
            Origin::Torrent { name, .. }
            | Origin::Debrid { name, .. }
            | Origin::File { name, .. } => name,
        }
    }

    pub fn length(&self) -> u64 {
        match self {
            Origin::Torrent { length, .. }
            | Origin::Debrid { length, .. }
            | Origin::File { length, .. } => *length,
        }
    }

    /// What identifies this file, so two viewers of the same one share a conversion.
    pub fn key(&self) -> String {
        match self {
            Origin::Torrent {
                info_hash,
                file_index,
                ..
            } => format!("t:{info_hash}:{file_index}"),
            Origin::Debrid {
                torrent_id,
                file_id,
                ..
            } => format!("d:{torrent_id}:{file_id}"),
            Origin::File { path, .. } => format!("f:{}", path.display()),
        }
    }

    /// A reader positioned at `offset`.
    pub async fn open_at(&self, offset: u64) -> anyhow::Result<Opened> {
        match self {
            Origin::Torrent {
                engine,
                info_hash,
                file_index,
                ..
            } => {
                let (_, mut reader, pin) = engine.stream(info_hash, *file_index).await?;
                if offset > 0 {
                    reader.seek(std::io::SeekFrom::Start(offset)).await?;
                }
                Ok(Opened {
                    reader: Box::new(reader),
                    _pin: Some(pin),
                })
            }
            Origin::Debrid {
                debrid,
                torrent_id,
                file_id,
                ..
            } => Ok(Opened {
                reader: debrid.open(*torrent_id, *file_id, offset).await?,
                _pin: None,
            }),
            Origin::File { path, .. } => {
                let mut file = tokio::fs::File::open(path).await?;
                if offset > 0 {
                    file.seek(std::io::SeekFrom::Start(offset)).await?;
                }
                Ok(Opened {
                    reader: Box::new(file),
                    _pin: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn file(bytes: &[u8]) -> (Origin, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "akidev-origin-{}-{}",
            std::process::id(),
            bytes.len()
        ));
        std::fs::write(&path, bytes).unwrap();
        let origin = Origin::File {
            path: path.clone(),
            length: bytes.len() as u64,
            name: "clip.mp4".into(),
        };
        (origin, path)
    }

    #[tokio::test]
    async fn a_reader_starts_at_the_offset_asked_for() {
        let (origin, path) = file(b"0123456789");
        let mut opened = origin.open_at(4).await.unwrap();
        let mut rest = Vec::new();
        opened.read_to_end(&mut rest).await.unwrap();
        assert_eq!(rest, b"456789");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn two_origins_for_one_file_share_a_key() {
        let a = Origin::File {
            path: "/x/y.mkv".into(),
            length: 1,
            name: "y.mkv".into(),
        };
        let b = Origin::File {
            path: "/x/y.mkv".into(),
            length: 1,
            name: "y.mkv".into(),
        };
        assert_eq!(a.key(), b.key());
    }
}
