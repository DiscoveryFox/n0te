//! What arrives from the network is untrusted: garbage frames, oversized lengths, messages
//! that name notebooks, sheets and blobs that don't exist (or `../`), a paired device gone
//! rogue. The engine must neither panic nor write outside its library, and keep syncing with
//! everyone else. Localhost only, no relays.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iroh::endpoint::presets;
use iroh::{Endpoint, RelayMode, SecretKey};
use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;
use n0te_sync::proto::{self, ALPN, Auth, DocKey, Live, LiveBrush, Msg, NbState, PairCode, SignedInvite};
use n0te_sync::{Engine, EngineOptions};

struct Rng(u64);
impl Rng {
    fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.u64() as u8).collect()
    }
}

fn root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("n0te-sync-hostile-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(22)))
}

fn engine(root: &Path, name: &str) -> Engine {
    let mut o = EngineOptions::new(name);
    o.local_only = true;
    Engine::start(root, o).expect("engine")
}

fn wait(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for: {what}");
}

#[test]
fn random_bytes_never_decode_into_a_panic() {
    let mut r = Rng(1);
    let valid = [
        proto::encode(&Msg::Announce { state: NbState { id: "nb".into(), index: vec![1, 2], sheets: vec![("s".into(), vec![3])] }, want_reply: true }),
        proto::encode(&Msg::BlobChunk { id: "b".into(), offset: 0, total: 10, data: vec![1; 10] }),
        proto::encode(&Live::Stroke { notebook: "n".into(), sheet: "s".into(), stroke: 1, seq: 2, brush: LiveBrush { width: 1.0, thinning: 0.0, color: 0, kind: 0 }, samples: vec![0; 32], done: true }),
    ];
    for _ in 0..5_000 {
        let n = (r.u64() % 200) as usize;
        let mut b = if r.u64() % 2 == 0 { valid[(r.u64() % 3) as usize].clone() } else { r.bytes(n) };
        if !b.is_empty() && r.u64() % 2 == 0 {
            let i = (r.u64() as usize) % b.len();
            b[i] = r.u64() as u8;
        }
        let _ = proto::decode::<Msg>(&b);
        let _ = proto::decode::<Live>(&b);
        let _ = proto::decode::<SignedInvite>(&b).map(|s| s.verify());
        let text = data_encoding_base64url(&b);
        let _ = PairCode::parse(&format!("n0te-pair:{text}"));
        let _ = SignedInvite::parse_link(&format!("n0te://invite/{text}")).map(|s| s.verify());
    }
}

/// Base64url without padding (what pair codes and invite links use), without a dependency.
fn data_encoding_base64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..c.len() + 1 {
            out.push(A[(n >> (18 - 6 * k) & 63) as usize] as char);
        }
    }
    out
}

#[test]
fn a_frame_announcing_gigabytes_is_refused_before_it_is_read() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let (mut a, mut b) = tokio::io::duplex(64);
        use tokio::io::AsyncWriteExt;
        a.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
        let t = Instant::now();
        assert!(proto::read_msg(&mut b).await.is_err());
        assert!(t.elapsed() < Duration::from_secs(1));
        // Within the limit but the sender stops: an error once the stream ends, no hang.
        a.write_all(&1000u32.to_be_bytes()).await.unwrap();
        a.write_all(&[0u8; 10]).await.unwrap();
        drop(a);
        assert!(proto::read_msg(&mut b).await.is_err());
    });
}

/// A device that pairs properly and then sends whatever it likes.
#[test]
fn a_rogue_paired_device_cannot_break_the_engine_or_escape_the_library() {
    let (ra, rb) = (root("victim"), root("friend"));
    let a = engine(&ra, "Laptop");
    let b = engine(&rb, "Tablet");
    // A real notebook on A, to see it survive.
    let lib = Library::open(&ra).unwrap();
    let nb = lib.create_notebook("Echt").unwrap();
    let (_, sheet) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
    lib.save_notebook(&nb).unwrap();
    let s = lib.load_sheet(&nb.id(), &sheet).unwrap();
    s.add_stroke(Brush::default(), &[InkPoint { x: 1.0, y: 1.0, pressure: 0.5, t_ms: 0 }, InkPoint { x: 9.0, y: 2.0, pressure: 0.5, t_ms: 8 }]).unwrap();
    lib.save_sheet(&nb.id(), &sheet, &s).unwrap();
    let real = nb.id();

    let code = PairCode::parse(&a.pair_code().unwrap()).unwrap();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    rt.block_on(async {
        let key = SecretKey::generate();
        let ep = Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled).bind_addr("127.0.0.1:0").unwrap().secret_key(key).alpns(vec![ALPN.to_vec()]).bind().await.unwrap();
        let conn = ep.connect(code.addr.clone(), ALPN).await.unwrap();
        let (mut send, mut recv) = conn.open_bi().await.unwrap();
        let proof = proto::pair_proof(&code.secret, &code.addr.id, &ep.id());
        proto::write_msg(&mut send, &Msg::Hello { proto: proto::PROTO, name: "Rogue".into(), addr: ep.addr(), auth: Auth::Pair { proof } }).await.unwrap();
        let welcome = proto::read_msg(&mut recv).await.unwrap();
        assert!(matches!(welcome, Msg::Welcome { .. }), "{welcome:?}");
        let bad_ids = ["..", "../..", "../../escape", "a/b", "", "nb\0", &"x".repeat(300), "ä"];
        let mut r = Rng(9);
        for id in bad_ids.iter().map(|s| s.to_string()).chain([real.clone()]) {
            let msgs = vec![
                Msg::Announce { state: NbState { id: id.clone(), index: r.bytes(40), sheets: (0..50).map(|k| (format!("{k}/../x"), r.bytes(20))).collect() }, want_reply: true },
                Msg::Updates { notebook: id.clone(), doc: DocKey::Index, data: r.bytes(300) },
                Msg::Updates { notebook: id.clone(), doc: DocKey::Sheet("../../evil".into()), data: r.bytes(300) },
                Msg::Updates { notebook: id.clone(), doc: DocKey::Sheet(sheet.clone()), data: r.bytes(300) },
                Msg::Updates { notebook: id.clone(), doc: DocKey::Index, data: Vec::new() },
                Msg::WantBlob { id: "../../../etc/passwd".into() },
                Msg::WantBlob { id: id.clone() },
                Msg::BlobChunk { id: id.clone(), offset: 0, total: u64::MAX, data: r.bytes(100) },
                Msg::BlobChunk { id: "0".repeat(64), offset: u64::MAX, total: 2 << 30, data: r.bytes(100) },
                Msg::NoBlob { id: id.clone() },
                Msg::Live(r.bytes(64)),
                Msg::Live(proto::encode(&Live::Erase { notebook: id.clone(), sheet: id.clone(), ids: vec![id.clone(); 100], dabs: vec![[f32::NAN; 3]; 100] })),
                Msg::Summon { notebook: id.clone(), sheet: id.clone(), viewport: [f32::NAN, f32::INFINITY, -0.0, f32::MAX] },
                Msg::Open { notebook: id.clone() },
                Msg::LatexResult { id: u64::MAX, latex: id.clone(), engine: String::new(), error: Some(id.clone()) },
            ];
            for m in msgs {
                let _ = proto::write_msg(&mut send, &m).await;
            }
            let _ = conn.send_datagram(r.bytes(200).into());
        }
        // A frame that doesn't decode ends this session (and only this one).
        let _ = send.write_all(&8u32.to_be_bytes()).await;
        let _ = send.write_all(&[0xFF; 8]).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        conn.close(0u32.into(), b"done");
        ep.close().await;
    });

    // Nothing landed outside the library, the real notebook is intact, A still syncs.
    assert!(!ra.parent().unwrap().join("escape").exists() && !ra.join("evil.loro").exists() && !ra.join("notebooks").join("evil.loro").exists());
    let lib = Library::open(&ra).unwrap();
    assert_eq!(lib.load_sheet(&real, &sheet).unwrap().strokes().len(), 1);
    assert!(lib.notebook_ids().unwrap().iter().all(|id| lib.load_notebook(id).is_ok()), "every notebook here still loads");
    let code = a.pair_code().unwrap();
    b.accept_pair(&code).unwrap();
    a.notify_local_change(&real);
    wait("the friend still gets the real notebook", || Library::open(&rb).ok().and_then(|l| l.load_sheet(&real, &sheet).ok()).is_some_and(|s| s.len() == 1));
    drop((a, b));
    let _ = std::fs::remove_dir_all(&ra);
    let _ = std::fs::remove_dir_all(&rb);
}

#[test]
fn strangers_get_a_reject_not_a_session() {
    let ra = root("stranger");
    let a = engine(&ra, "Laptop");
    let addr = a.addr();
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
    rt.block_on(async {
        let ep = Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled).bind_addr("127.0.0.1:0").unwrap().alpns(vec![ALPN.to_vec()]).bind().await.unwrap();
        for auth in [Auth::Own, Auth::Pair { proof: [0; 32] }, Auth::Guest { invite: vec![1, 2, 3] }, Auth::GuestPin { invite: Vec::new(), pin: "1234".into() }] {
            let conn = ep.connect(addr.clone(), ALPN).await.unwrap();
            let (mut send, mut recv) = conn.open_bi().await.unwrap();
            proto::write_msg(&mut send, &Msg::Hello { proto: proto::PROTO, name: "x".into(), addr: ep.addr(), auth }).await.unwrap();
            let reply = tokio::time::timeout(Duration::from_secs(5), proto::read_msg(&mut recv)).await.unwrap().unwrap();
            assert!(matches!(reply, Msg::Reject { .. }), "{reply:?}");
        }
        // Not even a hello: the engine drops it.
        let conn = ep.connect(addr.clone(), ALPN).await.unwrap();
        let (mut send, _recv) = conn.open_bi().await.unwrap();
        let _ = proto::write_msg(&mut send, &Msg::Shot).await;
        ep.close().await;
    });
    assert!(a.devices().is_empty() && a.status().guests.is_empty());
    drop(a);
    let _ = std::fs::remove_dir_all(&ra);
}
