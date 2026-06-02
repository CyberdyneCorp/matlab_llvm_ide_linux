//! Long-lived subprocess plumbing: spawns a child, reads its stdout/stderr on
//! background threads, and marshals each line back to the GTK main loop through
//! an `mpsc` channel polled by a `glib` timeout. The line-handling closures are
//! non-`Send` (they touch the `Rc`-based view models) and run only on the main
//! thread. Hosts the live REPL and DAP sessions.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use gtk::glib;

use matforge_core::services::dap::{DapClient, DapFramer};

/// Spawn a thread that reads `reader` line-by-line and forwards each (newline
/// trimmed) line over `tx`.
fn spawn_line_reader<R: Read + Send + 'static>(reader: R, tx: Sender<String>) {
    thread::spawn(move || {
        let mut buf = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match buf.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
                    if tx.send(trimmed).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Drain `rx` on the GTK main loop, calling `on_line` for each received line.
fn pump_to_main_loop(rx: Receiver<String>, mut on_line: impl FnMut(String) + 'static) {
    glib::timeout_add_local(Duration::from_millis(25), move || {
        while let Ok(line) = rx.try_recv() {
            on_line(line);
        }
        glib::ControlFlow::Continue
    });
}

// ---- Live REPL -------------------------------------------------------------

/// A running `matlabc -repl` process.
pub struct ReplSession {
    stdin: ChildStdin,
    child: Child,
}

impl ReplSession {
    /// Start `matlabc -repl` in `cwd`, forwarding every output line to `on_line`.
    pub fn start(
        matlabc: &Path,
        cwd: &Path,
        on_line: impl FnMut(String) + 'static,
    ) -> std::io::Result<ReplSession> {
        let mut child = Command::new(matlabc)
            .arg("-repl")
            .current_dir(cwd)
            // Make the matlab_plot runtime emit figures as ___MF_FIG_*___
            // sentinels so `plot(...)` in the REPL lands in the Plots panel.
            .env("MATLAB_LLVM_IDE_FIGURES", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdin = child.stdin.take().expect("piped stdin");

        let (tx, rx) = mpsc::channel::<String>();
        spawn_line_reader(stdout, tx.clone());
        spawn_line_reader(stderr, tx);
        pump_to_main_loop(rx, on_line);

        Ok(ReplSession { stdin, child })
    }

    /// Send a command, then the workspace-sync probe so the table refreshes.
    pub fn send(&mut self, command: &str) -> std::io::Result<()> {
        writeln!(self.stdin, "{command}")?;
        writeln!(
            self.stdin,
            "disp('___MF_WS_BEGIN___'); whos; disp('___MF_WS_END___')"
        )?;
        self.stdin.flush()
    }

    /// Send a command verbatim with no workspace-sync probe — used for IDE
    /// probes (e.g. value capture) that shouldn't trigger a `whos` refresh.
    pub fn eval(&mut self, command: &str) -> std::io::Result<()> {
        writeln!(self.stdin, "{command}")?;
        self.stdin.flush()
    }
}

impl Drop for ReplSession {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "exit");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---- DAP debug session -----------------------------------------------------

/// Synthetic body the reader emits when the adapter's stdout closes, so the
/// driver can tear the session down instead of hanging in "launching".
pub const DAP_EXIT: &str = "___MF_DAP_EXIT___";

/// A running `matlabc -dap` process plus the client-side protocol state.
pub struct DapSession {
    stdin: ChildStdin,
    child: Child,
    pub client: DapClient,
}

impl DapSession {
    /// Start `matlabc -dap <file>`, forwarding each decoded JSON body to
    /// `on_message` (already de-framed and on the main thread).
    pub fn start(
        matlabc: &Path,
        file: &Path,
        on_message: impl FnMut(String) + 'static,
    ) -> std::io::Result<DapSession> {
        let mut child = Command::new(matlabc)
            .arg("-dap")
            .arg(file)
            .env("MATLAB_LLVM_IDE_FIGURES", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stdin = child.stdin.take().expect("piped stdin");

        // A byte-level reader thread feeds the framer; complete bodies are sent
        // over the channel as whole strings.
        let (tx, rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut framer = DapFramer::new();
            let mut chunk = [0u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => {
                        // Adapter closed its stdout (clean exit or crash).
                        let _ = tx.send(DAP_EXIT.to_string());
                        break;
                    }
                    Ok(n) => {
                        for body in framer.feed(&chunk[..n]) {
                            if tx.send(body).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        });
        pump_to_main_loop(rx, on_message);

        Ok(DapSession { stdin, child, client: DapClient::new() })
    }

    /// Write a pre-framed request (built via `self.client`) to the adapter.
    pub fn write_frame(&mut self, frame: &str) -> std::io::Result<()> {
        self.stdin.write_all(frame.as_bytes())?;
        self.stdin.flush()
    }
}

impl Drop for DapSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
