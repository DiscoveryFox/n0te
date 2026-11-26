//! n0te sync engine: keeps notebooks the same on all of a person's devices and shares single
//! notebooks with guests — peer to peer over [iroh] (QUIC with hole punching, LAN direct,
//! encrypted relays as fallback; no server of our own).
//!
//! - **Own devices** are paired once with a pair code ([`Engine::pair_code`] /
//!   [`Engine::accept_pair`]) and then trust each other fully: every notebook syncs, both
//!   ways, as soon as they can reach each other ([`proto`] describes the protocol).
//! - **Guests** get a signed invite link for one notebook, read-only or read-write
//!   ([`Engine::create_invite`], [`Engine::join`], [`Engine::revoke`]).
//! - **Live**: presence (viewport, laser pointer), strokes while they are drawn, "open this
//!   notebook" and "follow me" requests ([`Engine::send_presence`] …), delivered as [`Event`]s.
//! - **History**: checkpoints of changed notebooks with thinning and restore-as-copy
//!   ([`history`]).
//!
//! The engine works on the same library directory as the notes app. Every write merges
//! with what is on disk under a lock ([`n0te_core::store::Library::save_notebook`]), and the
//! engine uses its own Loro peer id (`device.sync`), so the app and the engine can write at
//! the same time. The engine itself never edits documents — it only merges — except when
//! restoring a checkpoint as a new notebook.
//!
//! [iroh]: https://www.iroh.computer

pub mod history;
pub mod proto;

mod engine;
mod state;

pub use engine::{DeviceInfo, Engine, EngineOptions, GuestInfo, InviteInfo, InviteOptions, JoinedInfo, Status};
pub use state::Banned;

/// Android: lets iroh read the active network's DNS servers over JNI. Without it iroh falls
/// back to public nameservers, which restricted networks (libraries, eduroam) often block, so
/// the relays could not be found and only the LAN would work.
///
/// # Safety
/// `java_vm` must be the process's `JavaVM*`, `context` a JNI global reference to the
/// application context; both stay valid until the process exits. Call at most once.
#[cfg(target_os = "android")]
pub unsafe fn install_android_context(java_vm: *mut std::ffi::c_void, context: *mut std::ffi::c_void) {
    unsafe { iroh::dns::install_android_jni_context(java_vm, context) }
}
pub use proto::{LiveBrush, Role};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] n0te_core::error::Error),
    #[error("Netzwerk: {0}")]
    Net(String),
    #[error("Protokollfehler: {0}")]
    Protocol(String),
    #[error("{0}")]
    Invalid(String),
    #[error("abgelehnt: {0}")]
    Rejected(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// What happened (delivered to the listener / the events channel, on an engine thread).
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Another device changed a notebook here (reload `sheets`, and the outline if `index`).
    RemoteChanged { notebook: String, index: bool, sheets: Vec<String> },
    /// A blob (PDF) arrived.
    BlobArrived { id: String },
    DeviceOnline { device: DeviceInfo },
    DeviceOffline { id: String },
    /// A new own device was paired (on both sides).
    Paired { device: DeviceInfo },
    Presence { device: String, notebook: String, sheet: Option<String>, viewport: [f32; 4], zoom: f32, laser: Option<[f32; 2]> },
    LiveStroke { device: String, notebook: String, sheet: String, stroke: u64, seq: u32, brush: LiveBrush, samples: Vec<u8>, done: bool },
    /// An own device wants handwriting (PNG) as LaTeX (answer with `send_latex_result`).
    LatexRequest { device: String, id: u64, png: Vec<u8> },
    /// The LaTeX for an earlier `request_latex` (or the error).
    LatexResult { device: String, id: u64, latex: String, engine: String, error: Option<String> },
    /// An own device asks for a picture of this app's screen (answer with `send_screenshot`).
    ScreenshotRequest { device: String },
    /// The picture another device sent (JPEG).
    Screenshot { device: String, image: Vec<u8> },
    /// Another device is erasing: strokes it hid, area-eraser dabs (x, y, radius).
    LiveErase { device: String, notebook: String, sheet: String, ids: Vec<String>, dabs: Vec<[f32; 3]> },
    /// A laser pointer moving (x, y, t ms samples since the last one; `down` false when it
    /// lifts). `device` sent it here, `who` points (an own device, the owner or a guest, passed
    /// on by the owner), `guest` whether it points as a guest.
    Laser { device: String, notebook: String, sheet: String, who: String, samples: Vec<[f32; 3]>, down: bool, guest: bool },
    /// An own device asks to open a notebook (live session button).
    OpenRequest { device: String, notebook: String },
    /// An own device asks this one to follow (or stop following) its view.
    FollowRequest { device: String, on: bool },
    /// Owner side: a guest connected / left.
    /// `device`: the guest's id (its presence and live strokes carry it).
    GuestJoined { invite: String, notebook: String, name: String, device: String },
    GuestLeft { invite: String, notebook: String, device: String },
    /// Guest side: the owner changed our rights (read ↔ read-write).
    RoleChanged { notebook: String, write: bool },
    /// Guest side: the owner revoked (or let expire) our invite.
    AccessRevoked { notebook: String },
    /// Guest side: the connection to the owner of a joined notebook came up / went down.
    HostOnline { notebook: String, online: bool },
    /// Owner side: a new guest waits to be let in (`on`), or no longer waits (let in,
    /// turned away or gone). Answer with `Engine::admit`.
    GuestWaiting { invite: String, notebook: String, name: String, device: String, on: bool },
    /// Guest side, while joining: the owner has to let us in first.
    JoinWaiting { owner: String },
    /// The owner (or an own device) brings this view to `viewport` on `sheet` (sheet points).
    Summoned { device: String, notebook: String, sheet: String, viewport: [f32; 4] },
    SyncError { message: String },
}
