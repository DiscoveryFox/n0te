//! The wire protocol (ALPN `n0te/sync/1`).
//!
//! One bidirectional QUIC stream per connection carries [`Msg`] frames (`u32` big-endian
//! length + postcard body): the handshake, notebook state, document updates, blobs and small
//! requests. Presence and live strokes go as QUIC datagrams ([`Live`]); losing one only costs
//! a frame of the live view, the finished stroke always arrives as a document update.
//!
//! Sync is state-based: a side [`Msg::Announce`]s the version vectors of a notebook's docs;
//! the other side sends the updates the announcer lacks and, if it lacks something itself,
//! announces back once. Loro merges are idempotent, so duplicates are harmless.

use iroh::{EndpointAddr, EndpointId, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{Error, Result};

pub const ALPN: &[u8] = b"n0te/sync/1";
pub const PROTO: u32 = 1;
/// Largest frame (a full snapshot of a very dense sheet fits easily).
pub const MAX_FRAME: usize = 64 << 20;
/// Blob chunk size.
pub const CHUNK: usize = 256 << 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Auth {
    /// A paired own device (known by its endpoint id).
    Own,
    /// Pairing: proves knowledge of the one-time secret from the pair code.
    Pair { proof: [u8; 32] },
    /// A guest with a signed invite ([`SignedInvite`] bytes).
    Guest { invite: Vec<u8> },
    /// The same with the invite's PIN (added later: only sent when a PIN is given).
    GuestPin { invite: Vec<u8>, pin: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocKey {
    Index,
    Sheet(String),
}

/// A notebook's state on one side: version vectors (Loro encoding) of the index and of every
/// sheet that has a file. An empty `index` means "I don't have this notebook".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NbState {
    pub id: String,
    pub index: Vec<u8>,
    pub sheets: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Msg {
    Hello { proto: u32, name: String, addr: EndpointAddr, auth: Auth },
    /// Accepted. `scope`: for guests, the one notebook they may see.
    Welcome { name: String, scope: Option<String>, role: Option<Role> },
    Reject { reason: String },
    Announce { state: NbState, want_reply: bool },
    Updates { notebook: String, doc: DocKey, data: Vec<u8> },
    WantBlob { id: String },
    BlobChunk { id: String, offset: u64, total: u64, data: Vec<u8> },
    NoBlob { id: String },
    /// "Open this notebook" (live session button).
    Open { notebook: String },
    /// "Follow my view" on / off.
    Follow { on: bool },
    /// A live message that didn't fit in a datagram.
    Live(Vec<u8>),
    /// "Send me a picture of your n0te screen" (own devices only: testing without the tablet).
    Shot,
    /// The picture (JPEG).
    ShotData { image: Vec<u8> },
    /// "Turn this handwriting into LaTeX" (PNG; the laptop has the recognizers).
    LatexRequest { id: u64, png: Vec<u8> },
    /// The answer: LaTeX and which engine read it, or why it failed.
    LatexResult { id: u64, latex: String, engine: String, error: Option<String> },
    /// Owner to a new guest: "wait, the owner lets you in" (Welcome or Reject follows).
    Waiting { name: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Role {
    Read,
    ReadWrite,
}

/// Width, thinning, colour (0xAARRGGBB), kind — like the core's brush.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LiveBrush {
    pub width: f32,
    pub thinning: f32,
    pub color: u32,
    pub kind: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Live {
    Presence { notebook: String, sheet: Option<String>, viewport: [f32; 4], zoom: f32, laser: Option<[f32; 2]> },
    /// Samples (packed LE f32 x, y, pressure, t_ms) of a stroke being drawn, in sheet points,
    /// appended in `seq` order; `done` = pen lifted.
    Stroke { notebook: String, sheet: String, stroke: u64, seq: u32, brush: LiveBrush, samples: Vec<u8>, done: bool },
    /// Erasing as it happens (the stored result follows with the next save): whole strokes
    /// hidden by the stroke eraser, and/or area-eraser dabs (x, y, radius in sheet points).
    /// Added after v1: older peers can't decode it and ignore it.
    Erase { notebook: String, sheet: String, ids: Vec<String>, dabs: Vec<[f32; 3]> },
}

pub fn encode<T: Serialize>(v: &T) -> Vec<u8> {
    postcard::to_allocvec(v).expect("serializable")
}

pub fn decode<'a, T: Deserialize<'a>>(b: &'a [u8]) -> Result<T> {
    postcard::from_bytes(b).map_err(|e| Error::Protocol(format!("bad message: {e}")))
}

pub async fn write_msg(w: &mut (impl AsyncWriteExt + Unpin), m: &Msg) -> Result<()> {
    let body = encode(m);
    w.write_all(&(body.len() as u32).to_be_bytes()).await.map_err(|e| Error::Net(e.to_string()))?;
    w.write_all(&body).await.map_err(|e| Error::Net(e.to_string()))?;
    Ok(())
}

pub async fn read_msg(r: &mut (impl AsyncReadExt + Unpin)) -> Result<Msg> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await.map_err(|e| Error::Net(e.to_string()))?;
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(Error::Protocol(format!("frame of {len} bytes")));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await.map_err(|e| Error::Net(e.to_string()))?;
    decode(&body)
}

// --- pairing ------------------------------------------------------------------------------

/// What a pair code carries: where to reach the device that made it, and a one-time secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PairCode {
    pub addr: EndpointAddr,
    pub secret: [u8; 32],
    pub name: String,
}

const PAIR_PREFIX: &str = "n0te-pair:";

impl PairCode {
    pub fn to_text(&self) -> String {
        format!("{PAIR_PREFIX}{}", data_encoding::BASE64URL_NOPAD.encode(&encode(self)))
    }
    pub fn parse(s: &str) -> Result<Self> {
        let body = s.trim().strip_prefix(PAIR_PREFIX).ok_or_else(|| Error::Invalid("kein n0te-Kopplungscode".into()))?;
        let bytes = data_encoding::BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| Error::Invalid("Kopplungscode beschädigt".into()))?;
        decode(&bytes)
    }
}

/// Proof that the dialer knows the secret, bound to both endpoint ids (a relay or replay
/// can't reuse it for another pair of devices).
pub fn pair_proof(secret: &[u8; 32], code_maker: &EndpointId, dialer: &EndpointId) -> [u8; 32] {
    let mut msg = b"n0te-pair/1".to_vec();
    msg.extend_from_slice(code_maker.as_bytes());
    msg.extend_from_slice(dialer.as_bytes());
    *blake3::keyed_hash(secret, &msg).as_bytes()
}

// --- guest invites ------------------------------------------------------------------------

/// A capability: who may see which notebook how, until when. Signed by the owner's key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Invite {
    pub v: u32,
    pub id: String,
    pub notebook: String,
    pub title: String,
    pub role: Role,
    /// Unix seconds; 0 = never expires.
    pub expires: u64,
    pub owner: EndpointAddr,
    pub owner_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignedInvite {
    pub invite: Vec<u8>,
    pub sig: Vec<u8>,
}

const INVITE_PREFIX: &str = "n0te://invite/";

impl SignedInvite {
    pub fn sign(invite: &Invite, key: &SecretKey) -> Self {
        let bytes = encode(invite);
        let sig = key.sign(&bytes).to_bytes().to_vec();
        SignedInvite { invite: bytes, sig }
    }

    pub fn to_link(&self) -> String {
        format!("{INVITE_PREFIX}{}", data_encoding::BASE64URL_NOPAD.encode(&encode(self)))
    }

    pub fn parse_link(s: &str) -> Result<Self> {
        let body = s.trim().strip_prefix(INVITE_PREFIX).ok_or_else(|| Error::Invalid("kein n0te-Einladungslink".into()))?;
        let bytes = data_encoding::BASE64URL_NOPAD.decode(body.as_bytes()).map_err(|_| Error::Invalid("Einladungslink beschädigt".into()))?;
        decode(&bytes)
    }

    /// The invite, if the signature matches its own owner key (used by guests to reject a
    /// tampered link, and by the owner — who additionally checks the owner is itself).
    pub fn verify(&self) -> Result<Invite> {
        let invite: Invite = decode(&self.invite)?;
        let sig: [u8; 64] = self.sig.as_slice().try_into().map_err(|_| Error::Invalid("Signatur fehlt".into()))?;
        let key: PublicKey = invite.owner.id;
        key.verify(&self.invite, &Signature::from_bytes(&sig)).map_err(|_| Error::Invalid("Einladung ist nicht echt".into()))?;
        Ok(invite)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invites_sign_and_detect_tampering() {
        let key = SecretKey::generate();
        let inv = Invite { v: 1, id: "abc".into(), notebook: "nb".into(), title: "T".into(), role: Role::Read, expires: 0, owner: EndpointAddr::new(key.public()), owner_name: "Laptop".into() };
        let signed = SignedInvite::sign(&inv, &key);
        let back = SignedInvite::parse_link(&signed.to_link()).unwrap();
        assert_eq!(back.verify().unwrap(), inv);
        let mut forged = inv.clone();
        forged.role = Role::ReadWrite;
        let tampered = SignedInvite { invite: encode(&forged), sig: back.sig.clone() };
        assert!(tampered.verify().is_err());
    }

    #[test]
    fn pair_codes_round_trip() {
        let key = SecretKey::generate();
        let code = PairCode { addr: EndpointAddr::new(key.public()), secret: [7; 32], name: "Laptop".into() };
        assert_eq!(PairCode::parse(&code.to_text()).unwrap(), code);
        assert!(PairCode::parse("hallo").is_err());
        let other = SecretKey::generate().public();
        assert_ne!(pair_proof(&[7; 32], &key.public(), &other), pair_proof(&[8; 32], &key.public(), &other));
    }
}
