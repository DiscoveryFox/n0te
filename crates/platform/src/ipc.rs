//! The local connection between the n0te-sync service and the apps and the CLI: a byte
//! stream (n0te speaks JSON lines over it), one per client, readable and writable from two
//! threads at once ([`Stream::try_clone`]).
//!
//! - Unix: a Unix domain socket, `$XDG_RUNTIME_DIR/n0te-sync.sock` (see
//!   [`crate::paths::runtime_dir`]), mode 0600.
//! - Windows: a named pipe of this user, `\\.\pipe\n0te-sync-<user SID>` (plus a hash of the
//!   library when `$N0TE_LIBRARY` picks another one). Only this user and SYSTEM may open it,
//!   remote clients are refused, a second service can't take it over (first instance wins),
//!   and a client only talks to a pipe whose server runs as this user (so another account
//!   can't pose as the service by grabbing the name first).
//!
//! In both, [`Stream::connect`] fails at once when no service listens.

use std::fmt;
use std::io::{self, Read, Write};

/// Where the service listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    #[cfg(unix)]
    path: std::path::PathBuf,
    #[cfg(windows)]
    name: String,
}

impl Endpoint {
    /// The n0te-sync service of this user's library.
    pub fn service() -> Endpoint {
        #[cfg(unix)]
        {
            Endpoint { path: crate::paths::runtime_dir().join("n0te-sync.sock") }
        }
        #[cfg(windows)]
        {
            let user = crate::win::current_user().map(|u| u.text.clone()).unwrap_or_else(|_| std::env::var("USERNAME").unwrap_or_default());
            let mut name = format!(r"\\.\pipe\n0te-sync-{user}");
            // Another library (tests, a second library): its own service, so its own pipe.
            if std::env::var_os("N0TE_LIBRARY").is_some() {
                name.push_str(&format!("-{:016x}", fnv1a(crate::paths::library_root().as_os_str().as_encoded_bytes())));
            }
            Endpoint { name }
        }
    }

    /// The socket file (Unix).
    #[cfg(unix)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[cfg(unix)]
        {
            self.path.display().fmt(f)
        }
        #[cfg(windows)]
        {
            f.write_str(&self.name)
        }
    }
}

#[cfg(windows)]
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The service's end: accepts connections.
pub struct Listener(sys::Listener);

impl Listener {
    /// Listens at `ep`. Unix: a leftover socket file is replaced (the caller makes sure it is
    /// the only service, n0te-sync holds its library's lock) and the new one is owner-only.
    /// Windows: fails with [`io::ErrorKind::AddrInUse`] while another process serves the pipe.
    pub fn bind(ep: &Endpoint) -> io::Result<Listener> {
        sys::Listener::bind(ep).map(Listener)
    }

    /// Waits for the next client.
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(Stream)
    }

    /// Every client, as they come.
    pub fn incoming(&self) -> impl Iterator<Item = io::Result<Stream>> + '_ {
        std::iter::repeat_with(move || self.accept())
    }
}

/// One connection, from either end.
pub struct Stream(sys::Stream);

impl Stream {
    /// Connects to the service; fails at once ([`io::ErrorKind::NotFound`] or
    /// [`io::ErrorKind::ConnectionRefused`]) when none runs.
    pub fn connect(ep: &Endpoint) -> io::Result<Stream> {
        sys::Stream::connect(ep).map(Stream)
    }

    /// Another handle to the same connection (e.g. one thread reads while another writes).
    pub fn try_clone(&self) -> io::Result<Stream> {
        self.0.try_clone().map(Stream)
    }

    /// Ends the connection both ways, for every handle to it: reads see the end, writes fail.
    pub fn shutdown(&self) -> io::Result<()> {
        self.0.shutdown()
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ipc::Stream")
    }
}

#[cfg(unix)]
mod sys {
    use std::io::{self, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};

    use super::Endpoint;

    pub struct Listener(UnixListener);

    impl Listener {
        pub fn bind(ep: &Endpoint) -> io::Result<Listener> {
            let _ = std::fs::remove_file(&ep.path);
            let l = UnixListener::bind(&ep.path)?;
            let _ = crate::fs::make_private(&ep.path);
            Ok(Listener(l))
        }

        pub fn accept(&self) -> io::Result<Stream> {
            self.0.accept().map(|(s, _)| Stream(s))
        }
    }

    pub struct Stream(UnixStream);

    impl Stream {
        pub fn connect(ep: &Endpoint) -> io::Result<Stream> {
            UnixStream::connect(&ep.path).map(Stream)
        }
        pub fn try_clone(&self) -> io::Result<Stream> {
            self.0.try_clone().map(Stream)
        }
        pub fn shutdown(&self) -> io::Result<()> {
            self.0.shutdown(std::net::Shutdown::Both)
        }
        pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
        pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }
        pub fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }
}

/// Named pipes in overlapped mode: a handle opened for synchronous I/O lets only one call at a
/// time through, so a thread blocked reading would hold up every write from another thread
/// (the GUI's reader and its live commands, the service's session writer). Overlapped
/// handles don't serialize; every call here still blocks until its own I/O is done.
#[cfg(windows)]
mod sys {
    use std::io;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, GetLastError, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile};
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::CreateEventW;
    use windows_sys::core::BOOL;

    use super::Endpoint;
    use crate::win::{Handle, current_user, process_user, wide};

    const BUFFER: u32 = 64 * 1024;

    /// Runs one overlapped call and waits for it: `start` gets the OVERLAPPED to pass on.
    fn overlapped(h: &Handle, start: impl FnOnce(*mut OVERLAPPED) -> BOOL) -> io::Result<u32> {
        // Manual reset, as overlapped I/O wants it.
        let event = Handle::new(unsafe { CreateEventW(null(), 1, 0, null()) }).ok_or_else(io::Error::last_os_error)?;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = event.0;
        if start(&mut ov) == 0 {
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
        }
        // Done at once or pending: either way the result comes from here (`ov` lives until then).
        let mut n = 0u32;
        if unsafe { GetOverlappedResult(h.0, &ov, &mut n, 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n)
    }

    fn is(e: &io::Error, code: u32) -> bool {
        e.raw_os_error() == Some(code as i32)
    }

    struct Pipe {
        h: Handle,
        /// The service's end (it can disconnect the client).
        server: bool,
        closed: AtomicBool,
    }

    // A kernel handle may be used from any thread; overlapped calls on it don't block each other.
    unsafe impl Send for Pipe {}
    unsafe impl Sync for Pipe {}

    /// A security descriptor from `ConvertStringSecurityDescriptorToSecurityDescriptorW`.
    struct Descriptor(PSECURITY_DESCRIPTOR);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }

    pub struct Listener {
        name: Vec<u16>,
        sd: Descriptor,
        /// The instance clients connect to next: there is always one, so between two clients the
        /// pipe never vanishes (a client would take that for "no service").
        next: Mutex<Option<Pipe>>,
    }

    // The descriptor is only read (by CreateNamedPipeW); the rest is Send + Sync.
    unsafe impl Send for Listener {}
    unsafe impl Sync for Listener {}

    impl Listener {
        pub fn bind(ep: &Endpoint) -> io::Result<Listener> {
            let me = current_user()?;
            // Only this user and SYSTEM; "P": nothing inherited.
            let sddl = wide(format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", me.text));
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut l = Listener { name: wide(&ep.name), sd: Descriptor(sd), next: Mutex::new(None) };
            let first = l.instance(true)?;
            l.next = Mutex::new(Some(first));
            Ok(l)
        }

        fn instance(&self, first: bool) -> io::Result<Pipe> {
            let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: self.sd.0, bInheritHandle: 0 };
            let mut open = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
            if first {
                open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
            }
            let h = unsafe { CreateNamedPipeW(self.name.as_ptr(), open, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, BUFFER, BUFFER, 0, &sa) };
            match Handle::new(h) {
                Some(h) => Ok(Pipe { h, server: true, closed: AtomicBool::new(false) }),
                None => {
                    let e = io::Error::last_os_error();
                    if first && is(&e, ERROR_ACCESS_DENIED) {
                        return Err(io::Error::new(io::ErrorKind::AddrInUse, "another process serves this pipe"));
                    }
                    Err(e)
                }
            }
        }

        pub fn accept(&self) -> io::Result<Stream> {
            let mut next = self.next.lock().unwrap_or_else(|p| p.into_inner());
            let pipe = match next.take() {
                Some(p) => p,
                None => self.instance(false)?,
            };
            let connected = wait_for_client(&pipe.h);
            // The next instance before this one is handed out.
            *next = self.instance(false).ok();
            connected.map(|_| Stream(Arc::new(pipe)))
        }
    }

    /// `ConnectNamedPipe` in overlapped mode, waited for.
    fn wait_for_client(h: &Handle) -> io::Result<()> {
        let event = Handle::new(unsafe { CreateEventW(null(), 1, 0, null()) }).ok_or_else(io::Error::last_os_error)?;
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = event.0;
        if unsafe { ConnectNamedPipe(h.0, &mut ov) } != 0 {
            return Ok(());
        }
        match unsafe { GetLastError() } {
            // The client came between CreateNamedPipeW and ConnectNamedPipe: nothing pending.
            ERROR_PIPE_CONNECTED => Ok(()),
            ERROR_IO_PENDING => {
                let mut n = 0u32;
                if unsafe { GetOverlappedResult(h.0, &ov, &mut n, 1) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
            }
            // E.g. ERROR_NO_DATA: the client came and went already.
            err => Err(io::Error::from_raw_os_error(err as i32)),
        }
    }

    pub struct Stream(Arc<Pipe>);

    impl Stream {
        pub fn connect(ep: &Endpoint) -> io::Result<Stream> {
            let name = wide(&ep.name);
            let mut busy = 0;
            let h = loop {
                // SECURITY_IDENTIFICATION: the server may learn who we are, not act as us.
                let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_READ | GENERIC_WRITE, 0, null(), OPEN_EXISTING, FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, null_mut()) };
                if let Some(h) = Handle::new(h) {
                    break h;
                }
                let e = io::Error::last_os_error();
                // Every instance taken for a moment (the service makes the next one right away).
                if is(&e, ERROR_PIPE_BUSY) && busy < 10 {
                    busy += 1;
                    unsafe { WaitNamedPipeW(name.as_ptr(), 500) };
                    continue;
                }
                // ERROR_FILE_NOT_FOUND (NotFound): no service.
                return Err(e);
            };
            // Only a service of this user: another account may have grabbed the name first.
            let mut pid = 0u32;
            if unsafe { GetNamedPipeServerProcessId(h.0, &mut pid) } == 0 {
                return Err(io::Error::last_os_error());
            }
            if process_user(pid)?.sid != current_user()?.sid {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("{} belongs to another user", ep.name)));
            }
            Ok(Stream(Arc::new(Pipe { h, server: false, closed: AtomicBool::new(false) })))
        }

        pub fn try_clone(&self) -> io::Result<Stream> {
            Ok(Stream(self.0.clone()))
        }

        pub fn shutdown(&self) -> io::Result<()> {
            let p = &self.0;
            p.closed.store(true, Ordering::SeqCst);
            // Wakes calls blocked on other threads; the service's end also drops the client
            // (its reads end, its writes fail).
            unsafe { CancelIoEx(p.h.0, null()) };
            if p.server {
                unsafe { DisconnectNamedPipe(p.h.0) };
            }
            Ok(())
        }

        pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let p = &self.0;
            if p.closed.load(Ordering::SeqCst) || buf.is_empty() {
                return Ok(0);
            }
            let len = buf.len().min(u32::MAX as usize) as u32;
            match overlapped(&p.h, |ov| unsafe { ReadFile(p.h.0, buf.as_mut_ptr(), len, null_mut(), ov) }) {
                Ok(n) => Ok(n as usize),
                // The other end closed (or was disconnected): end of stream.
                Err(e) if is(&e, ERROR_BROKEN_PIPE) || is(&e, ERROR_PIPE_NOT_CONNECTED) => Ok(0),
                Err(e) if is(&e, ERROR_OPERATION_ABORTED) && p.closed.load(Ordering::SeqCst) => Ok(0),
                Err(e) => Err(e),
            }
        }

        pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let p = &self.0;
            if p.closed.load(Ordering::SeqCst) {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }
            if buf.is_empty() {
                return Ok(0);
            }
            let len = buf.len().min(u32::MAX as usize) as u32;
            overlapped(&p.h, |ov| unsafe { WriteFile(p.h.0, buf.as_ptr(), len, null_mut(), ov) }).map(|n| n as usize)
        }

        pub fn flush(&mut self) -> io::Result<()> {
            // Writes are done once they returned (the data sits in the pipe).
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::time::{Duration, Instant};

    use super::*;

    /// An endpoint of its own per test (Unix: in a temp folder; Windows: a pipe name).
    fn test_endpoint(tag: &str) -> Endpoint {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let unique = format!("n0te-ipc-test-{tag}-{}-{nanos}", std::process::id());
        #[cfg(unix)]
        {
            let dir = std::env::temp_dir().join(&unique);
            std::fs::create_dir_all(&dir).unwrap();
            Endpoint { path: dir.join("s.sock") }
        }
        #[cfg(windows)]
        {
            Endpoint { name: format!(r"\\.\pipe\{unique}") }
        }
    }

    #[test]
    fn nothing_listening_fails_at_once() {
        let ep = test_endpoint("none");
        let t = Instant::now();
        assert!(Stream::connect(&ep).is_err());
        assert!(t.elapsed() < Duration::from_secs(2));
        #[cfg(unix)]
        let _ = std::fs::remove_dir_all(ep.path().parent().unwrap());
    }

    #[test]
    fn request_reply_and_a_duplex_session() {
        let ep = test_endpoint("duplex");
        let listener = Listener::bind(&ep).unwrap();
        let server = std::thread::spawn(move || {
            // 1: one request, one reply, then closed by dropping.
            let conn = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(conn.try_clone().unwrap()).read_line(&mut line).unwrap();
            let mut w = conn;
            writeln!(w, "reply to {}", line.trim()).unwrap();
            drop(w);
            // 2: a session: a writer thread sends while this thread is blocked reading.
            let conn = listener.accept().unwrap();
            let mut w = conn.try_clone().unwrap();
            let writer = std::thread::spawn(move || {
                for i in 0..3 {
                    writeln!(w, "event {i}").unwrap();
                }
            });
            let mut got = Vec::new();
            for l in BufReader::new(conn.try_clone().unwrap()).lines() {
                got.push(l.unwrap());
            }
            writer.join().unwrap();
            conn.shutdown().unwrap();
            got
        });

        let mut c = Stream::connect(&ep).unwrap();
        writeln!(c, "ping").unwrap();
        let mut line = String::new();
        BufReader::new(c).read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "reply to ping");

        let c = Stream::connect(&ep).unwrap();
        let mut events = BufReader::new(c.try_clone().unwrap()).lines();
        // Reading on this thread, writing from another, both on the same connection.
        let mut w = c.try_clone().unwrap();
        let sender = std::thread::spawn(move || {
            writeln!(w, "live 1").unwrap();
            writeln!(w, "live 2").unwrap();
        });
        for i in 0..3 {
            assert_eq!(events.next().unwrap().unwrap(), format!("event {i}"));
        }
        sender.join().unwrap();
        drop(c);
        drop(events);
        assert_eq!(server.join().unwrap(), vec!["live 1".to_string(), "live 2".to_string()]);
        #[cfg(unix)]
        let _ = std::fs::remove_dir_all(ep.path().parent().unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn a_second_service_cannot_take_the_pipe() {
        let ep = test_endpoint("twice");
        let _first = Listener::bind(&ep).unwrap();
        assert_eq!(Listener::bind(&ep).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));
    }

    #[cfg(windows)]
    #[test]
    fn the_service_pipe_is_per_user() {
        let ep = Endpoint::service();
        assert!(ep.name.starts_with(r"\\.\pipe\n0te-sync-S-1-"), "{ep}");
    }
}
