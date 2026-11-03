# Aurora-Viewer LEAP Studio (GUI)

A modern, high-performance graphical desktop application built in **Rust** using [`serde-llsd`](https://github.com/John-Nagle/serde-llsd) and [`eframe` / `egui`](https://github.com/emilk/egui) to communicate with, test, and control all Linden Lab LEAP (LLSD Event-based API Protocol) features in [Aurora-Viewer](https://github.com/Aurora-Viewer/Aurora-Viewer).

> 📖 **[Read the Complete User & Architecture Guide (GUIDE.md)](GUIDE.md)** for detailed documentation on camera sequencers, radar minimap, blackbox recorder, Rhai scripting engine, and the 20 MCP tools.  
> 📐 **[Read the LLSD Protocol & Async Architecture Plan (LLSD_PROTOCOL_AND_ASYNC_PLAN.md)](LLSD_PROTOCOL_AND_ASYNC_PLAN.md)** for protocol specification, non-blocking threading design, and subsystem milestone roadmap.

---

> [!NOTE]
> ### ⚠️ Project Status: Early Rough Draft & Diagnostic Test Harness
> Please note that this is a **first rough draft** with potential errors and an interface that is messy and not organized well. It was designed as a **quick-and-dirty testing system** for developing and verifying Aurora-Viewer LEAP features, so don't expect it to be beautiful, amazing, or error-free at this point!
>
> - **Fork & Customize**: Feel free to fork the repository and do whatever you want with it yourself if there's something you don't like.
> - **Issues & Feedback**: You are welcome to post GitHub issues and I will look at them and try to fix them as they come up, provided they are in scope of what I want the tool to be.

---

## Key Features

### 🚀 1. Automated Feature Verification Suite
- **1-Click "Run All Feature Tests"**: Sequentially executes the full suite of verification tests against the running viewer binary.
- **Visual Progress & Test Cards**: Real-time progress bar, individual test execution durations, and expandable assertion logs.
- **Covers All Features**:
  1. Handshake UUIDs & protocol negotiation
  2. Ping / Pong with verbatim numeric & string `reqid` correlation
  3. API Introspection (`getAPIs` discovery)
  4. Operation Schema Inspection (`getAPI`)
  5. `LLViewerControl` mutations (`get`, `set`, `toggle`, `groups`, `vars`)
  6. Subsystem Telemetry (`LLAppViewer.status`, `LLAgent`, `LLLogin`, `LLCamera`, `LLInventory`)
  7. Pub/Sub pump lifecycle (`listen`, duplicate rejection, `stoplistening`)
  8. `LLChat.sendChat` transmission and event routing
  9. Complex payload parsing (nested maps, arrays, unicode strings)
  10. High-throughput pipelining stress burst (25 rapid back-to-back requests)

### 🔍 2. API Introspection (`LLLeapListener`)
- Live directory of all registered subsystem APIs discovered via `getAPIs`.
- Search & filter APIs by name or description.
- Detailed operation inspector: View operation names, descriptions, required parameters, and optional parameters.
- Instant ping testing tool with roundtrip latency timing.

### ⚙️ 3. Viewer Configuration (`LLViewerControl`)
- Real-time settings browser querying all configuration variables via `vars`.
- Categorized by groups (`Display`, `Graphics`, `General`).
- Instant interactive mutations:
  - 1-click **Toggle** for boolean settings (`VSync`, `Shadows`, etc.).
  - Inline input & **Apply** for numbers and strings (`DrawDistance`, `Language`, etc.).

### 💬 4. Interactive Chat Console (`LLChat`)
- Send chat directly into the simulator with channel selection (Channel 0 for public chat, custom channels for scripting).
- Configurable chat type: Whisper (0), Normal (1), Shout (2).
- Live scrolling chat log showing sent messages and incoming chat broadcasts forwarded from the `chat` pump.

### 🧭 5. Avatar, Camera & Inventory Telemetry (`LLAgent`, `LLCamera`, `LLInventory`)
- **Avatar (`LLAgent`)**: Live avatar UUID and region coordinates `(X, Y, Z)`.
- **Camera (`LLCamera`)**: Camera eye position, focus target, field of view (FOV), and a "Reset Camera View" button.
- **Inventory (`LLInventory`)**: Root folder UUID and owner UUID queries.

### 🔐 6. Application Lifecycle & Session (`LLAppViewer` & `LLLogin`)
- Runtime diagnostics: Viewer version, active screen (`Login`, `World`), uptime counter, and frame count.
- Session status: Login state (`STATE_LOGIN_SHOW`, etc.) and authenticated status.
- "Logout" button.
- "Request Graceful Quit" (`LLAppViewer.requestQuit`) and "Force Kill Process".

### 📡 7. Live Event Stream & Pub/Sub Monitor
- Dynamic listener registration: Subscribe to any pump (`chat`, `teleport`, `StartupState`, `settings`, `im`, `login`) via `listen`.
- Active subscriptions manager with 1-click unsubscribe (`stoplistening`).
- Live event feed with timestamp, source pump, and expandable pretty-printed LLSD Notation.
- Filter events by text query or target pump name.

### 🛠️ 8. Raw LLSD Console
- Directly write, edit, and transmit custom LLSD Notation envelopes parsed with `serde-llsd`.
- Syntax validator with instant error reporting.
- Ready-to-use presets (Ping, Settings queries, Chat, Camera).
- Request/response inspector with latency timing.

### 📜 9. Viewer Console Output (`stderr`)
- Live streaming console log capturing all stderr messages emitted by `aurora-viewer.exe`.
- Auto-scroll to bottom toggle, search filter, and clear log options.

---

## Architecture & `serde-llsd` Integration

The app uses `serde-llsd` (`0.3.6`) for full serialization and deserialization of the LEAP protocol wire format:

```text
[Aurora-Viewer LEAP Studio GUI]
        │
        ├── codec.rs: Length-prefixed framing (<len>:<notation>) via serde-llsd
        │
        ├── client.rs: Child process launcher & background stdio threads (crossbeam)
        │       ├── Stdin Writer: Transmits framed notation bytes
        │       ├── Stdout Reader: Decodes frames and dispatches by reqid / event queue
        │       └── Stderr Reader: Streams console output into a thread-safe ring buffer
        │
        ├── tests.rs: Automated 10-point test runner
        │
        └── ui/: Immediate-mode GUI panels using eframe / egui
```

---

## Running the Application

### 1. Launch via Cargo
```powershell
cd "d:\AI code projects\antigravity\aurora-viewer\aurora-leap-gui"
cargo run
```

### 2. Or run the compiled standalone executable:
```powershell
& "d:\AI code projects\antigravity\aurora-viewer\aurora-leap-gui\target\debug\aurora-leap-gui.exe"
```

Once opened:
1. Confirm the **Viewer Binary** path points to `..\Aurora-Viewer\target\debug\aurora-viewer.exe` (or use the **Browse...** button).
2. Click **"▶ Launch & Connect"** at the top right.
3. Once the status pill turns green (`🟢 Connected`), switch between the tabs to interact with all viewer subsystems or click **"▶ Run All Feature Tests"** in the **Feature Tests** tab!
