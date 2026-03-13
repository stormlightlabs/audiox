# Tauri 2 Architecture

## Process Model

Tauri apps run two processes:

- **Core process** (Rust): owns the application lifecycle, manages windows, handles IPC, and runs backend logic. Has full OS access.
- **WebView process**: renders the frontend (HTML/CSS/JS) via the platform's native webview engine (WKWebView on macOS, WebView2 on Windows, WebKitGTK on Linux). Sandboxed — no direct filesystem or network access.

Communication between the two is exclusively via IPC (Tauri commands and events).

## IPC: Commands and Events

**Commands** are request/response RPC calls from frontend to backend:

```rust
#[tauri::command]
async fn greet(name: String) -> Result<String, String> {
    Ok(format!("Hello, {name}!"))
}
```

Frontend invokes via `invoke("greet", { name: "Alice" })`. Commands are async by default and run on the Tokio runtime.

**Events** are fire-and-forget messages in either direction:

- Backend → frontend: `app.emit("progress", payload)` broadcasts to all windows. `window.emit("local-event", payload)` targets one.
- Frontend → backend: `emit("my-event", payload)` with `app.listen("my-event", handler)` on the Rust side.

## State Management (Rust side)

Tauri's managed state is dependency-injected into commands:

```rust
app.manage(MyState::new());

#[tauri::command]
fn read_state(state: tauri::State<'_, MyState>) -> String { ... }
```

State must be `Send + Sync`. Wrap interior mutability in `Mutex` or `RwLock`.

## Plugin System

Tauri 2 plugins encapsulate reusable backend + frontend functionality:

1. Rust crate implements `tauri::plugin::Builder` — registers commands, manages state, hooks into lifecycle events.
2. JS/TS package wraps `invoke()` calls with typed APIs.
3. Capabilities (permissions) declare what OS APIs the plugin needs.

## Sidecars (External Binaries)

Tauri bundles external executables via `bundle.externalBin` in `tauri.conf.json`. At runtime, `app.shell().sidecar("name")` spawns the process with stdout/stderr streaming. Sidecars are platform-specific — the build system selects the correct binary by target triple suffix (e.g., `ffmpeg-aarch64-apple-darwin`).

## Security Model

The capability-based permission system restricts what the frontend can do:

- Each window is assigned a set of **capabilities** (JSON files in `src-tauri/capabilities/`).
- Capabilities grant specific permissions: `fs:read`, `shell:execute`, `dialog:open`, etc.
- By default, the frontend cannot access any OS APIs — every capability must be explicitly granted.

## Example

```rust
use tauri::Manager;

#[tauri::command]
fn load_document_title(state: tauri::State<'_, AppState>, id: String) -> Result<String, String> {
    state.lookup_title(&id).ok_or_else(|| format!("document {id} not found"))
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![load_document_title])
        .setup(|app| {
            app.emit("startup://ready", true).map_err(std::io::Error::other)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```
