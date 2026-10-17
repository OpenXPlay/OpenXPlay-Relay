// SPDX-FileCopyrightText: DiPlay and xcertplay contributors
// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: GPL-3.0-only
// Derived from DiPlay (https://github.com/shihabal3amri/DiPlay), based on xcertplay (https://github.com/shilapi/xcertplay). Modified by OpenXPlay.org on 2026-10-15.

use crate::*;

/// `Iap2ArtworkTransfer` protocol data.
#[derive(Clone, Debug)]
pub struct Iap2ArtworkTransfer {
    pub id: u8,

    pub bytes: Vec<u8>,
}
/// `FileTransferOutcome` protocol data.
#[derive(Clone, Debug)]
pub struct FileTransferOutcome {
    pub transfer: Option<Iap2ArtworkTransfer>,

    pub diagnostic: Option<String>,
    pub replies: Vec<Vec<u8>>,
}
/// State owned by `Iap2FileTransferReceiver`; construct through its factory.
pub struct Iap2FileTransferReceiver {
    maximum: usize,
    pending: std::collections::VecDeque<(u8, usize, Vec<u8>)>,
}
impl Iap2FileTransferReceiver {
    pub fn new(maximum_artwork_bytes: usize) -> Self {
        assert!(maximum_artwork_bytes > 0);
        Self {
            maximum: maximum_artwork_bytes,
            pending: Default::default(),
        }
    }

    pub fn accept(&mut self, datagram: &[u8]) -> Result<FileTransferOutcome> {
        let mut out = FileTransferOutcome {
            transfer: None,
            diagnostic: None,
            replies: Vec::new(),
        };
        if datagram.len() < 2 {
            return Ok(out);
        }
        let id = datagram[0];
        let flags = datagram[1];
        match flags & 15 {
            4 => {
                self.pending.retain(|p| p.0 != id);
                if datagram.len() < 12 || u16::from_be_bytes([datagram[10], datagram[11]]) != 2 {
                    out.replies.push(vec![id, 2]);
                    return Ok(out);
                }
                let size = u64::from_be_bytes(datagram[2..10].try_into().unwrap());
                if size > self.maximum as u64 {
                    out.replies.push(vec![id, 2]);
                    out.transfer = Some(Iap2ArtworkTransfer {
                        id,
                        bytes: Vec::new(),
                    });
                } else if size == 0 {
                    out.replies = vec![vec![id, 1], vec![id, 5]];
                    out.transfer = Some(Iap2ArtworkTransfer {
                        id,
                        bytes: Vec::new(),
                    });
                } else {
                    self.pending.push_back((id, size as usize, Vec::new()));
                    if self.pending.len() > 4 {
                        self.pending.pop_front();
                    }
                    out.replies.push(vec![id, 1]);
                }
            }
            0 => {
                let Some(index) = self.pending.iter().position(|p| p.0 == id) else {
                    return Ok(out);
                };
                let p = &mut self.pending[index];
                let count = datagram.len() - 2;
                if count > p.1 - p.2.len() {
                    self.pending.remove(index);
                    out.replies.push(vec![id, 2]);
                    out.transfer = Some(Iap2ArtworkTransfer {
                        id,
                        bytes: Vec::new(),
                    });
                } else {
                    p.2.extend(&datagram[2..]);
                    if flags & 0x40 != 0 || p.2.len() >= p.1 {
                        let (_, expected, bytes) = self.pending.remove(index).unwrap();
                        if bytes.len() != expected {
                            out.replies.push(vec![id, 2]);
                            out.transfer = Some(Iap2ArtworkTransfer {
                                id,
                                bytes: Vec::new(),
                            });
                        } else {
                            out.replies.push(vec![id, 5]);
                            out.transfer = Some(Iap2ArtworkTransfer { id, bytes });
                        }
                    }
                }
            }
            2 if self.pending.iter().any(|p| p.0 == id) => {
                self.pending.retain(|p| p.0 != id);
                out.transfer = Some(Iap2ArtworkTransfer {
                    id,
                    bytes: Vec::new(),
                });
            }
            _ => {}
        }
        Ok(out)
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Protocol message, field, or framing constant.
pub const HEADER_BYTES: u64 = 2;
/// Protocol message, field, or framing constant.
pub const SIZE_OFFSET: u64 = 2;
/// Protocol message, field, or framing constant.
pub const FILE_TYPE_OFFSET: u64 = 10;
/// Protocol message, field, or framing constant.
pub const VERSION_2_SETUP_BYTES: u64 = 12;
/// Protocol message, field, or framing constant.
pub const OPCODE_MASK: u64 = 0x0f;
/// Protocol message, field, or framing constant.
pub const FLAG_LAST: u64 = 0x40;
/// Protocol message, field, or framing constant.
pub const OPCODE_DATA: u64 = 0;
/// Protocol message, field, or framing constant.
pub const OPCODE_START: u64 = 1;
/// Protocol message, field, or framing constant.
pub const OPCODE_CANCEL: u64 = 2;
/// Protocol message, field, or framing constant.
pub const OPCODE_SETUP: u64 = 4;
/// Protocol message, field, or framing constant.
pub const OPCODE_SUCCESS: u64 = 5;
/// Protocol message, field, or framing constant.
pub const NOW_PLAYING_ARTWORK_TYPE: u64 = 2;
/// Protocol message, field, or framing constant.
pub const MAXIMUM_PENDING_TRANSFERS: u64 = 4;

/// Constant expression: `2 * 1024 * 1024`.
pub const DEFAULT_MAXIMUM_ARTWORK_BYTES: u64 = 2097152;
