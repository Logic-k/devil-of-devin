use anyhow::Context;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::Deserialize;
use serde_json::json;
use std::io::{Read, Write};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// A ConPTY session (e.g. running the `devin` TUI).
pub struct PtySession {
    #[allow(dead_code)]
    pub id: String,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    pair_master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
}

#[derive(Debug, Deserialize)]
pub struct PtySpawnOpts {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub env: Option<std::collections::HashMap<String, String>>,
    /// When true, the reader auto-answers terminal queries (DSR `\x1b[6n` etc.)
    /// — needed for headless TUI subprocesses with no real terminal attached.
    pub respond_to_queries: Option<bool>,
}

impl PtySession {
    pub fn spawn(app: AppHandle, id: String, opts: PtySpawnOpts) -> anyhow::Result<Arc<Self>> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: opts.rows.unwrap_or(30),
                cols: opts.cols.unwrap_or(120),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("openpty")?;

        let mut cmd = CommandBuilder::new(&opts.command);
        if let Some(args) = &opts.args {
            cmd.args(args.iter().map(|s| s.as_str()));
        }
        if let Some(cwd) = &opts.cwd {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        if let Some(env) = &opts.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        let child = pair.slave.spawn_command(cmd).context("spawn pty")?;
        let writer = pair.master.take_writer().context("pty writer")?;
        let mut reader = pair.master.try_clone_reader().context("pty reader")?;

        let session = Arc::new(PtySession {
            id: id.clone(),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            pair_master: Arc::new(Mutex::new(pair.master)),
        });

        // reader thread → base64 chunks to frontend (+ optional query answers)
        {
            let app = app.clone();
            let id = id.clone();
            let auto = opts.respond_to_queries.unwrap_or(false);
            let session2 = session.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let mut tail: Vec<u8> = Vec::new();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if auto {
                                let mut scan = tail.clone();
                                scan.extend_from_slice(&buf[..n]);
                                const QUERIES: &[(&[u8], &[u8])] = &[
                                    (b"\x1b[6n", b"\x1b[1;1R"),
                                    (b"\x1b[5n", b"\x1b[0n"),
                                    (b"\x1b[0c", b"\x1b[?1;2c"),
                                    (b"\x1b[c", b"\x1b[?1;2c"),
                                ];
                                for (pat, resp) in QUERIES {
                                    while let Some(pos) =
                                        scan.windows(pat.len()).position(|w| w == *pat)
                                    {
                                        let _ = session2.write(resp);
                                        for b in &mut scan[pos..pos + pat.len()] {
                                            *b = b' ';
                                        }
                                    }
                                }
                                tail = scan[scan.len().saturating_sub(8)..].to_vec();
                            }
                            let _ = app.emit(
                                &format!("pty://out/{id}"),
                                json!({ "data": base64_encode(&buf[..n]) }),
                            );
                        }
                    }
                }
                let _ = app.emit(&format!("pty://exit/{id}"), json!({}));
            });
        }

        Ok(session)
    }

    pub fn write(&self, data: &[u8]) -> anyhow::Result<()> {
        let mut w = self.writer.lock();
        w.write_all(data)?;
        w.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.pair_master.lock().resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&self) -> anyhow::Result<()> {
        self.child.lock().kill()?;
        Ok(())
    }

    /// True once the child has exited (reaps the status).
    pub fn exited(&self) -> bool {
        self.child
            .lock()
            .try_wait()
            .map(|s| s.is_some())
            .unwrap_or(true)
    }
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

// ---------- commands ----------

#[tauri::command]
pub fn pty_spawn(
    state: tauri::State<'_, crate::state::AppState>,
    app: AppHandle,
    opts: PtySpawnOpts,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let s = PtySession::spawn(app, id.clone(), opts).map_err(|e| e.to_string())?;
    state.ptys.lock().insert(id.clone(), s);
    Ok(id)
}

#[tauri::command]
pub fn pty_write(
    state: tauri::State<'_, crate::state::AppState>,
    id: String,
    data: String,
) -> Result<(), String> {
    let ptys = state.ptys.lock();
    let s = ptys.get(&id).ok_or("no pty")?;
    s.write(data.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(
    state: tauri::State<'_, crate::state::AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let ptys = state.ptys.lock();
    let s = ptys.get(&id).ok_or("no pty")?;
    s.resize(cols, rows).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_kill(state: tauri::State<'_, crate::state::AppState>, id: String) -> Result<(), String> {
    if let Some(s) = state.ptys.lock().remove(&id) {
        s.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}
