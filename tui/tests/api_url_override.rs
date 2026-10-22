#![allow(clippy::expect_used)]
//! `ESTELLE_API_URL` must move the REAL binary's requests, and an invalid value must stop them.
//!
//! 🔑 DISCRIMINATING PAIR. The two cases below cannot both pass unless the override is wired through
//! `Client::production` — the constructor every shipped entry point uses. If the env var were read
//! nowhere, the first case's listener would receive nothing (the request would go to production); if it
//! were read but not validated, the second case would not refuse. No production request is ever sent:
//! the first case answers locally, and the second refuses before any socket is opened.

use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::process::Command;
use std::process::Output;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long the local listener waits for the binary's first request.
const ACCEPT_WINDOW: Duration = Duration::from_secs(60);
/// Upper bound on the request head we read back.
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const TEST_KEY: &str = "estelle_live_api-url-override-test-only";

fn run_estelle(api_url: &str, cwd: &std::path::Path) -> Output {
    let binary = codex_utils_cargo_bin::cargo_bin("estelle").expect("locate built Estelle binary");
    Command::new(binary)
        .args(["recall", "--repo", "acme/widgets", "anything"])
        .current_dir(cwd)
        .env("ESTELLE_API_URL", api_url)
        .env("ESTELLE_API_KEY", TEST_KEY)
        .env_remove("ESTELLE_KEY")
        .env_remove("ESTELLE_RECEIPT_PATH")
        .output()
        .expect("run Estelle")
}

/// Accept one connection, return its request head, answer `200 {}`.
fn serve_one(listener: TcpListener, tx: mpsc::Sender<String>) {
    let Ok((mut stream, _)) = listener.accept() else {
        return;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut buffer = vec![0_u8; MAX_REQUEST_BYTES];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let _ = tx.send(String::from_utf8_lossy(&buffer[..read]).into_owned());
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}",
    );
}

#[test]
fn a_localhost_override_receives_the_binarys_authenticated_request() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local listener");
    let port = listener.local_addr().expect("listener address").port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || serve_one(listener, tx));
    let cwd = tempfile::tempdir().expect("isolated cwd");

    let _output = run_estelle(&format!("http://127.0.0.1:{port}"), cwd.path());

    let head = rx
        .recv_timeout(ACCEPT_WINDOW)
        .expect("the override must route the real binary's request to the local listener");
    let request_line = head.lines().next().unwrap_or_default();
    assert!(request_line.ends_with("HTTP/1.1"), "{request_line}");
    assert!(
        head.to_ascii_lowercase()
            .contains(&format!("authorization: bearer {TEST_KEY}")),
        "the local API must receive the environment credential"
    );
}

#[test]
fn a_remote_plain_http_override_is_refused_and_never_falls_back_to_production() {
    let cwd = tempfile::tempdir().expect("isolated cwd");
    let output = run_estelle("http://10.255.255.1:9", cwd.path());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "{combined}");
    assert!(combined.contains("ESTELLE_API_URL"), "{combined}");
    assert!(!combined.contains(TEST_KEY), "the key must never be echoed");
}
