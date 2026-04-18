//! VSOCK / Unix dial-and-pipe for the MCP shim runtime.
//!
//! When `mcp2cli` is invoked via an `mcp-<server>-<tool>` symlink
//! and the server's tool cache declares a `vsock_port`, this module
//! opens a connection to the MCP bridge and pipes NDJSON frames both
//! ways:
//!
//! - stdin  → socket   (the MCP request written by the caller)
//! - socket → stdout   (the MCP response written back to the caller)
//!
//! The transport is pluggable at dial time:
//!
//! - **AF_VSOCK** (production) — dial `(cid, port)`. Matches the
//!   in-guest scenario where `mcp2cli` runs inside the VM and the
//!   proxy runs on the host.
//! - **AF_UNIX** (dev/CI) — dial a Unix socket at `<dir>/<server>.sock`.
//!   Selected when the `MCP_SHIM_UNIX_DIR` environment variable is
//!   set; keeps integration tests hermetic without requiring a
//!   `vhost_vsock` kernel module.
//!
//! Current scope:
//!
//! - Dial + bidirectional NDJSON pipe.
//! - Half-close semantics: after stdin EOF the writer half of the
//!   socket is shut down, giving the proxy a clean signal to close.

#![allow(unsafe_code)]

use anyhow::{Context, Result, anyhow};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;

/// How the shim should reach the MCP bridge.
#[derive(Debug, Clone)]
pub enum DialTarget {
    /// AF_VSOCK — production. Host CID is typically 2.
    Vsock { cid: u32, port: u32 },
    /// AF_UNIX — dev/CI.
    Unix(PathBuf),
}

/// Resolve a dial target for a given `(server, vsock_port)` from
/// environment variables, matching the same pattern the rest of the
/// sandbox stack uses:
///
/// - `MCP_SHIM_UNIX_DIR` set → `Unix(<dir>/<server>.sock)` wins.
/// - `MCP_HOST_CID` set → `Vsock { cid = MCP_HOST_CID, port = vsock_port }`.
/// - Neither set → `None`; caller falls back to the "not wired" error.
pub fn target_from_env(server: &str, vsock_port: u32) -> Option<DialTarget> {
    if let Some(dir) = std::env::var_os("MCP_SHIM_UNIX_DIR") {
        let mut p = PathBuf::from(dir);
        p.push(format!("{server}.sock"));
        return Some(DialTarget::Unix(p));
    }
    if let Some(cid) = std::env::var("MCP_HOST_CID")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        return Some(DialTarget::Vsock {
            cid,
            port: vsock_port,
        });
    }
    None
}

// ---------------- AF_VSOCK dial ----------------

const AF_VSOCK: libc::c_int = 40;

#[repr(C)]
struct SockaddrVm {
    svm_family: u16,
    svm_reserved1: u16,
    svm_port: u32,
    svm_cid: u32,
    svm_flags: u8,
    svm_zero: [u8; 3],
}

fn dial_vsock(cid: u32, port: u32) -> Result<OwnedFd> {
    // SAFETY: socket()/connect() are plain FFI; return codes checked;
    // fd wrapped in OwnedFd immediately so it cannot leak.
    let raw = unsafe { libc::socket(AF_VSOCK, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(anyhow!(
            "socket(AF_VSOCK): {}",
            std::io::Error::last_os_error()
        ));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let addr = SockaddrVm {
        svm_family: AF_VSOCK as u16,
        svm_reserved1: 0,
        svm_port: port,
        svm_cid: cid,
        svm_flags: 0,
        svm_zero: [0; 3],
    };
    let rc = unsafe {
        libc::connect(
            raw,
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<SockaddrVm>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(anyhow!(
            "connect(AF_VSOCK cid={cid} port={port}): {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(fd)
}

/// A connected stream — either Unix or VSOCK — exposing `Read`+`Write`+`try_clone`.
#[derive(Debug)]
pub enum DialStream {
    Unix(UnixStream),
    Vsock(OwnedFd),
}

impl DialStream {
    pub fn try_clone(&self) -> Result<DialStream> {
        match self {
            DialStream::Unix(u) => Ok(DialStream::Unix(u.try_clone()?)),
            DialStream::Vsock(fd) => {
                let dup = fd.try_clone().context("dup(AF_VSOCK fd)")?;
                Ok(DialStream::Vsock(dup))
            }
        }
    }

    pub fn shutdown_write(&self) -> Result<()> {
        match self {
            DialStream::Unix(u) => Ok(u.shutdown(std::net::Shutdown::Write)?),
            DialStream::Vsock(fd) => {
                let rc = unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_WR) };
                if rc != 0 {
                    let err = std::io::Error::last_os_error();
                    if err.raw_os_error() != Some(libc::ENOTCONN) {
                        return Err(anyhow!("shutdown(SHUT_WR): {err}"));
                    }
                }
                Ok(())
            }
        }
    }
}

impl Read for DialStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            DialStream::Unix(u) => u.read(buf),
            DialStream::Vsock(fd) => {
                let n =
                    unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
                if n < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(n as usize)
            }
        }
    }
}

impl Write for DialStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            DialStream::Unix(u) => u.write(buf),
            DialStream::Vsock(fd) => {
                let n = unsafe { libc::write(fd.as_raw_fd(), buf.as_ptr() as *const _, buf.len()) };
                if n < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(n as usize)
            }
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            DialStream::Unix(u) => u.flush(),
            DialStream::Vsock(_) => Ok(()),
        }
    }
}

/// Open a connection according to `target`.
pub fn dial(target: &DialTarget) -> Result<DialStream> {
    match target {
        DialTarget::Unix(path) => {
            let s = UnixStream::connect(path)
                .with_context(|| format!("connect unix socket {}", path.display()))?;
            Ok(DialStream::Unix(s))
        }
        DialTarget::Vsock { cid, port } => Ok(DialStream::Vsock(dial_vsock(*cid, *port)?)),
    }
}

// ---------------- NDJSON pipe ----------------

/// One-shot MCP request/response over an open dial.
///
/// Writes `request` as a single NDJSON line, reads back exactly one
/// line, parses it as JSON, half-closes the write side so the proxy's
/// child sees EOF and the proxy can tear down the session cleanly,
/// then returns the parsed response.
///
/// This is the `tools/call` shape the shim marshalling needs: the
/// shim builds the request from argv, sends it, gets one response,
/// exits. Falls back to [`pipe_ndjson`] for the raw-pipe escape hatch.
pub fn single_shot(request: &serde_json::Value, stream: DialStream) -> Result<serde_json::Value> {
    let mut stream_read = stream.try_clone().context("clone stream for read half")?;
    let mut stream_write = stream;

    // Write request + newline.
    let mut bytes = serde_json::to_vec(request).context("serialise request")?;
    bytes.push(b'\n');
    stream_write
        .write_all(&bytes)
        .context("write request to mcp bridge")?;
    stream_write.flush().ok();
    stream_write
        .shutdown_write()
        .context("half-close write side after request")?;

    // Read one line back.
    let mut reader = std::io::BufReader::new(&mut stream_read);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .context("read response from mcp bridge")?;
    if n == 0 {
        return Err(anyhow!("mcp bridge closed connection without responding"));
    }
    serde_json::from_str(&line).context("parse response JSON")
}

/// Pipe NDJSON both ways between `stdin`/`stdout` and an open
/// connection. Returns after the reader side (socket → stdout) has
/// seen EOF — that is the proxy's signal that the session is done.
///
/// Implementation: two OS threads, one per direction, joined before
/// return. After stdin EOF we `shutdown_write` the socket so the
/// proxy's `read_line` can resolve to 0 and it can tear down the child.
pub fn pipe_ndjson<R: Read + Send + 'static, W: Write + Send + 'static>(
    stdin: R,
    mut stdout: W,
    stream: DialStream,
) -> Result<()> {
    let mut stream_read = stream.try_clone().context("clone stream for read half")?;
    let stream_write_handle = stream;

    let writer = thread::spawn(move || -> Result<()> {
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        let mut stream = stream_write_handle;
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // Caller's stdin closed → half-close the write side.
                    let _ = stream.shutdown_write();
                    return Ok(());
                }
                Ok(_) => {
                    stream
                        .write_all(line.as_bytes())
                        .context("write to mcp bridge")?;
                    stream.flush().ok();
                }
                Err(e) => return Err(anyhow!("read from stdin: {e}")),
            }
        }
    });

    // Reader side runs on this thread so stdout flushes eagerly.
    let mut reader = BufReader::new(&mut stream_read);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                stdout
                    .write_all(line.as_bytes())
                    .context("write to stdout")?;
                stdout.flush().ok();
            }
            Err(e) => return Err(anyhow!("read from mcp bridge: {e}")),
        }
    }

    // Don't propagate writer-thread errors after the session ended —
    // they're usually just EPIPE as the proxy tears down the child.
    let _ = writer.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::time::Duration;

    // Env-var tests share process state; serialize them with a mutex
    // so parallel test execution doesn't interleave mutations.
    use std::sync::Mutex;
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<F: FnOnce()>(f: F) {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            std::env::remove_var("MCP_SHIM_UNIX_DIR");
            std::env::remove_var("MCP_HOST_CID");
        }
        f();
        unsafe {
            std::env::remove_var("MCP_SHIM_UNIX_DIR");
            std::env::remove_var("MCP_HOST_CID");
        }
    }

    #[test]
    fn target_from_env_prefers_unix_dir() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("MCP_SHIM_UNIX_DIR", "/tmp/shim-test");
                std::env::set_var("MCP_HOST_CID", "2");
            }
            let t = target_from_env("fs", 6001).unwrap();
            match t {
                DialTarget::Unix(p) => {
                    assert_eq!(p, PathBuf::from("/tmp/shim-test/fs.sock"));
                }
                other => panic!("expected Unix, got {other:?}"),
            }
        });
    }

    #[test]
    fn target_from_env_falls_back_to_vsock() {
        with_clean_env(|| {
            unsafe {
                std::env::set_var("MCP_HOST_CID", "2");
            }
            let t = target_from_env("fs", 6001).unwrap();
            match t {
                DialTarget::Vsock { cid, port } => {
                    assert_eq!(cid, 2);
                    assert_eq!(port, 6001);
                }
                other => panic!("expected Vsock, got {other:?}"),
            }
        });
    }

    #[test]
    fn target_from_env_returns_none_without_config() {
        with_clean_env(|| {
            assert!(target_from_env("fs", 6001).is_none());
        });
    }

    #[test]
    fn pipe_ndjson_round_trips_via_unix_echo() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("echo.sock");
        let listener = UnixListener::bind(&sock).unwrap();

        // Server: read one line, echo it back, close.
        let server = thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(conn.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            conn.write_all(line.as_bytes()).unwrap();
            // drop: client EOFs on read.
        });

        // Give server a moment (bind is sync so this is mostly paranoia).
        thread::sleep(Duration::from_millis(10));
        let stream = dial(&DialTarget::Unix(sock.clone())).unwrap();

        // Drive pipe with a cursor stdin + Vec stdout.
        let stdin = std::io::Cursor::new(b"{\"hello\":\"world\"}\n".to_vec());
        let mut stdout_buf: Vec<u8> = Vec::new();

        // Wrap the Vec so we can reclaim it after pipe_ndjson returns.
        struct Writer<'a>(&'a mut Vec<u8>);
        impl std::io::Write for Writer<'_> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.flush()
            }
        }

        {
            let writer = Writer(&mut stdout_buf);
            // Move semantics — types for the generic bounds.
            // `Writer` is 'a; pipe_ndjson requires 'static. Use a
            // second approach: give pipe a stdout clone via Vec<u8>
            // directly.
            drop(writer);
        }

        // Simpler: pass an owned Vec as stdout and extract after join.
        // Since pipe_ndjson needs `W: 'static`, use an mpsc channel
        // to reclaim the writer. Keep it tight:
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer = Collector {
            buf: Vec::new(),
            tx: Some(tx),
        };
        pipe_ndjson(stdin, writer, stream).unwrap();
        server.join().unwrap();
        let out = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("collector should deliver");
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.trim(), r#"{"hello":"world"}"#);
    }

    struct Collector {
        buf: Vec<u8>,
        tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    }

    impl std::io::Write for Collector {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buf.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Drop for Collector {
        fn drop(&mut self) {
            if let Some(tx) = self.tx.take() {
                let _ = tx.send(std::mem::take(&mut self.buf));
            }
        }
    }

    #[test]
    fn dial_unix_errors_when_socket_missing() {
        let err = dial(&DialTarget::Unix(PathBuf::from(
            "/tmp/definitely-not-here.sock",
        )))
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("unix socket"), "got: {msg}");
    }
}
