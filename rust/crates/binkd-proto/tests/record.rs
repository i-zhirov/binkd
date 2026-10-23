//! The replay of a recorded session.
//!
//! The recordings live in `tests/sessions/` of the repository. A recording holds
//! the bytes of both directions of one session. The test replays the bytes of the
//! peer into a fresh session and compares every byte that our side sends with the
//! recording.
//!
//! The C binary records nothing of the kind: its `binlog.c` holds the statistics
//! of a session, not the bytes. The recordings of this directory come from the
//! Rust session itself and from a live session of the C binary.
//!
//! Set `BINKD_C` to the path of the C binary to run the test of a recorded C
//! session. Without the variable the test reports that it is skipped.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use binkd_ftn::addr::FtnAddr;
use binkd_proto::record::{Recorder, Recording, Replay};
use binkd_proto::{run, Report, Role, SendFile, SessionConfig};

/// The number of the bytes of the file of a test.
const FILE_SIZE: usize = 300;

/// Returns the directory of the recorded sessions.
fn sessions_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/sessions")
}

/// Makes a directory for the spool of a test.
/// The settings of the spool of a test.
fn spool(dir: &Path) -> binkd_spool::config::SpoolConfig {
    // The inbound directory of the partial files.
    std::fs::create_dir_all(dir.join("in")).unwrap();
    let mut spool = binkd_spool::config::SpoolConfig::new();
    // The free-space values of the C defaults, `init_config()`.
    spool.minfree = -1;
    spool.minfree_nonsecure = -1;
    spool.inbound_nonsecure = dir.join("in").display().to_string();
    spool
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("binkd-record-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("in")).unwrap();
    std::fs::create_dir_all(dir.join("out")).unwrap();
    dir
}

fn address(s: &str) -> FtnAddr {
    let table = binkd_ftn::domain::DomainTable::new(Vec::new());
    FtnAddr::parse(s, &table).unwrap()
}

/// The settings of one side of a recording.
///
/// The values must match the ones of the recording: the banner and the `M_FILE`
/// frames hold them. The answerer of the recording of the C binary holds the
/// address that the C client calls, so the C code finds its AKA.
fn config(dir: &Path, role: Role) -> SessionConfig {
    config_with(dir, role, "2:5047/100@fidonet")
}

/// The settings of one side of a recording with the address of the answerer.
fn config_with(dir: &Path, role: Role, server_aka: &str) -> SessionConfig {
    let (sysname, aka) = match role {
        Role::Client => ("Rust client", "2:5047/13@fidonet"),
        Role::Server => ("Rust server", server_aka),
    };
    SessionConfig {
        sysname: sysname.to_string(),
        sysop: "Rust sysop".to_string(),
        location: "Rust location".to_string(),
        nodeinfo: "Rust node".to_string(),
        addresses: vec![address(aka)],
        password: String::new(),
        inbound: dir.join("in"),
        spool: spool(dir),
        version: "rust-test/0.1".to_string(),
        blksize: 4096,
        zminsize: 1024,
        zlevel: 0,
        zrules: Vec::new(),
        methods: binkd_proto::Methods::none(),
        crypt: false,
        nr: false,
        nd: false,
        domains: binkd_ftn::domain::DomainTable::new(Vec::new()),
        max_servers: 100,
        servers: 1,
        trace: false,
        rates: Vec::new(),
        bw_send: -100,
        bw_recv: -100,
        skip: Vec::new(),
        listed: false,
    }
}

/// Makes the file that a recording sends.
fn make_file(dir: &Path) -> SendFile {
    let path = dir.join("out/13b7000d.pkt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, vec![b'r'; FILE_SIZE]).unwrap();
    SendFile {
        netname: "13b7000d.pkt".to_string(),
        path: Some(path),
        size: FILE_SIZE as u64,
        time: 1_000_000_000,
    }
}

/// Replays a recording and returns the bytes of our side.
async fn replay(recording: &Recording, dir: &Path, server_aka: &str) -> (Vec<u8>, Report) {
    use binkd_spool::inbound::{FreeSpace, Inbound, TFile};

    // The partial file that the answerer of the recording held.
    if !recording.partial.is_empty() {
        let tfile = TFile {
            netname: "13b7000d.pkt".to_string(),
            size: FILE_SIZE as u64,
            time: 1_000_000_000,
            start: 0,
        };
        let spool = spool(dir);
        let inbound = Inbound {
            dir: dir.join("in").display().to_string(),
            secure: true,
            fa: vec![address(server_aka)],
            nfa: 1,
            skip_all_flag: false,
        };
        let mut rnd = || 7u32;
        let mut file = inbound
            .inb_fopen(&tfile, &spool, FreeSpace::default(), 0, &mut rnd)
            .expect("the partial file");
        std::io::Write::write_all(&mut file, &recording.partial).unwrap();
        drop(file);
    }
    let files = match recording.role {
        Role::Client => vec![make_file(dir)],
        Role::Server => Vec::new(),
    };
    let mut config = config_with(dir, recording.role, server_aka);
    config.nr = recording.nr;
    config.nd = recording.nd;
    let mut replay = Replay::new(recording);
    let report = run(
        &mut replay,
        recording.role,
        &config,
        &files,
        &mut |_| Ok(()),
        Duration::from_secs(5),
    )
    .await
    .expect("the replayed session failed");
    (replay.sent().to_vec(), report)
}

/// Replays every recorded session of the repository.
#[tokio::test]
async fn replays_the_recorded_sessions() {
    let mut found = 0;
    let entries = std::fs::read_dir(sessions_dir()).expect("cannot read tests/sessions");
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("session") {
            continue;
        }
        let recording = Recording::read(&path).expect("cannot read a recording");
        let dir = temp_dir("replay");
        let (sent, report) = replay(&recording, &dir, "2:5047/100@fidonet").await;
        assert_eq!(
            sent.len(),
            recording.sent.len(),
            "the session of {} sent {} bytes, the recording holds {}",
            path.display(),
            sent.len(),
            recording.sent.len()
        );
        assert_eq!(
            sent,
            recording.sent,
            "the session of {} differs from the recording",
            path.display()
        );
        match recording.role {
            Role::Client => assert_eq!(report.files_sent, 1, "{}", path.display()),
            Role::Server => assert_eq!(report.files_received, 1, "{}", path.display()),
        }
        found += 1;
    }
    assert!(found > 0, "no recording in {}", sessions_dir().display());
}

/// Returns the path of the C binary, or `None` when the variable is not set.
fn c_binary() -> Option<String> {
    std::env::var("BINKD_C")
        .ok()
        .filter(|p| Path::new(p).is_file())
}

/// The C client polls the Rust server and the recorder keeps the session.
async fn record_a_c_binary_session(dir: &Path, port: u16) -> Recording {
    let c_config = format!(
        "domain fidonet outb 2\n\
         address 2:5047/100@fidonet\n\
         sysname \"C client\"\n\
         sysop \"C sysop\"\n\
         location \"C location\"\n\
         nodeinfo \"C node\"\n\
         inbound in\n\
         log c-client.log\n\
         loglevel 6\n\
         conlog 0\n\
         node 2:5047/13@fidonet 127.0.0.1:{port} secret\n\
         zallow *\n"
    );
    std::fs::write(dir.join("binkd.conf"), c_config).unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    let server_config = config_with(
        &temp_dir("c-record-server"),
        Role::Server,
        "2:5047/13@fidonet",
    );
    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut recorder = Recorder::new(stream, Role::Server);
        let result = run(
            &mut recorder,
            Role::Server,
            &server_config,
            &[],
            &mut |_| Ok(()),
            Duration::from_secs(20),
        )
        .await;
        assert!(result.is_ok(), "{result:?}");
        recorder.into_recording()
    };

    let client_dir = dir.to_path_buf();
    let client = tokio::task::spawn_blocking(move || {
        let client = Command::new(c_binary().unwrap())
            .args(["-P", "2:5047/13@fidonet", "-p", "binkd.conf"])
            .current_dir(&client_dir)
            .status()
            .expect("cannot run the C client");
        assert!(client.success(), "the C client failed");
    });

    let (recording, client) = tokio::join!(server, client);
    client.unwrap();
    recording
}

/// Replays the bytes of a live session of the C binary.
///
#[tokio::test]
async fn replays_a_session_of_the_c_binary() {
    let Some(_) = c_binary() else {
        eprintln!("skip: set BINKD_C to the path of the C binary");
        return;
    };

    let dir = temp_dir("c-binary");
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let recording = record_a_c_binary_session(&dir, port).await;
    assert!(!recording.sent.is_empty());

    // The replay of the recording sends the same bytes.
    let dir = temp_dir("c-binary-replay");
    let (sent, report) = replay(&recording, &dir, "2:5047/13@fidonet").await;
    assert_eq!(sent, recording.sent, "the replay differs from the session");
    assert_eq!(report.files_received, 0);
}

/// Records the two ends of a Rust session.
///
/// The test is ignored: it writes the recordings of `tests/sessions/` of the
/// repository. Run it after a change of the banner or of a frame.
#[ignore = "writes the recordings of tests/sessions"]
#[tokio::test]
async fn write_the_recordings() {
    std::fs::create_dir_all(sessions_dir()).expect("cannot create tests/sessions");
    for (role, modes, partial) in [
        (Role::Client, false, false),
        (Role::Server, false, false),
        (Role::Client, true, false),
        (Role::Server, true, false),
        // The answerer holds the first part of the file of the caller.
        (Role::Server, true, true),
    ] {
        let dir = temp_dir("write");
        let mut side = config(&dir, role);
        side.nr = modes;
        side.nd = modes;
        // The side that calls sends the file, as in the other tests.
        let files = match role {
            Role::Client => vec![make_file(&dir)],
            Role::Server => Vec::new(),
        };
        let peer_files = match role {
            Role::Client => Vec::new(),
            Role::Server => vec![make_file(&dir)],
        };
        // The partial file of the answerer, the port of `inb_fopen()`.
        let held: Vec<u8> = if partial {
            use binkd_spool::inbound::{FreeSpace, Inbound, TFile};

            let content = vec![b'r'; FILE_SIZE];
            let tfile = TFile {
                netname: "13b7000d.pkt".to_string(),
                size: FILE_SIZE as u64,
                time: 1_000_000_000,
                start: 0,
            };
            let spool = spool(&dir);
            // The inbound of the session, as the settings hold it.
            let inbound = Inbound {
                dir: dir.join("in").display().to_string(),
                secure: true,
                fa: vec![address("2:5047/100@fidonet")],
                nfa: 1,
                skip_all_flag: false,
            };
            let mut rnd = || 7u32;
            let mut file = inbound
                .inb_fopen(&tfile, &spool, FreeSpace::default(), 0, &mut rnd)
                .expect("the partial file");
            std::io::Write::write_all(&mut file, &content[..100]).unwrap();
            drop(file);
            content[..100].to_vec()
        } else {
            Vec::new()
        };
        let (left, right) = tokio::io::duplex(1 << 20);
        let (mut recorder, peer) = match role {
            Role::Client => (Recorder::new(left, role), right),
            Role::Server => (Recorder::new(right, role), left),
        };
        let ours = async {
            run(
                &mut recorder,
                role,
                &side,
                &files,
                &mut |_| Ok(()),
                Duration::from_secs(5),
            )
            .await
        };
        let theirs = async {
            run(
                peer,
                match role {
                    Role::Client => Role::Server,
                    Role::Server => Role::Client,
                },
                &{
                    let mut peer = config(
                        &dir,
                        match role {
                            Role::Client => Role::Server,
                            Role::Server => Role::Client,
                        },
                    );
                    peer.nr = modes;
                    peer.nd = modes;
                    peer
                },
                &peer_files,
                &mut |_| Ok(()),
                Duration::from_secs(5),
            )
            .await
        };
        let (ours, theirs) = tokio::join!(ours, theirs);
        assert!(ours.is_ok() && theirs.is_ok());
        let mut recording = recorder.into_recording();
        recording.nr = modes;
        recording.nd = modes;
        recording.partial = held;
        let name = match (role, modes, partial) {
            (Role::Client, false, _) => "rust-client.session",
            (Role::Server, false, _) => "rust-server.session",
            (Role::Client, true, _) => "rust-client-modes.session",
            (Role::Server, true, false) => "rust-server-modes.session",
            (Role::Server, true, true) => "rust-server-resumed.session",
        };
        recording
            .write(&sessions_dir().join(name))
            .expect("cannot write a recording");
        eprintln!("wrote {name}");
    }
}
