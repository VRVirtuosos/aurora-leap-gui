# Aurora-Viewer LEAP Studio User Guide

## Overview
**Aurora-Viewer LEAP Studio** (`aurora-leap-gui`) is a comprehensive Rust desktop application built using `eframe`/`egui` and powered by [`serde-llsd`](https://crates.io/crates/serde-llsd). It provides a full GUI, automated scripting environment, and standard Model Context Protocol (MCP) server for controlling **Aurora-Viewer** over the Linden Event-driven Architecture Protocol (LEAP).

> [!IMPORTANT]
> **Required Viewer Branch**: You must run the **`llsd`** branch of Aurora-Viewer ([https://github.com/VRVirtuosos/Aurora-Viewer/tree/llsd](https://github.com/VRVirtuosos/Aurora-Viewer/tree/llsd)) to use this tool, as it contains all the necessary LEAP pumps, telemetry feeds, and asynchronous handlers.

---

## Core Features & Subsystems

### 1. 🔑 Avatar Login Panel (`tab_login.rs`)
Direct authentication and grid dispatching over LEAP without interacting with the viewer's native login UI:
- **Username & Masked Password**: Credentials are protected on screen with password masking bullets.
- **Grid Presets**: Second Life Main Grid (Agni), Second Life Beta (Aditi), OSGrid, or custom OpenSim grid URIs.
- **Start Location**: Options to resume at `Last location`, `Home`, or custom SLURLs.
- **Session Telemetry**: Real-time state polling (`STATE_LOGIN_SHOW`, `STATE_LOGIN_WAIT`, `STATE_STARTED`).

### 2. 🕹️ Avatar Movement Controls (`tab_movement.rs`)
Interactive direction pad and coordinate teleportation interface:
- **3×3 D-Pad**: Forward, Backward, Strafe Left/Right, Turn Left/Right, Jump/Fly Up, Crouch/Fly Down.
- **Step Size Slider**: Discrete increments from 0.5 m to 20.0 m.
- **Flight Toggle**: Instant toggle between walking and flying (`LLAgent.move` with `"fly"`).
- **Coordinate Teleportation**: Direct XYZ inputs and preset coordinates (`Centre (128, 128, 25)`, `Corner (0, 0, 50)`, `Far (256, 256, 25)`).

### 3. 🎥 Camera Optics, Controls & Location Bookmarks (`tab_camera.rs`)
Advanced 3D camera staging, framing presets, and persistent bookmarks:
- **Manual Optics**:
  - 3D Pan D-Pad (Forward, Backward, Strafe Left/Right, Elevation Up/Down).
  - Orbit Yaw & Pitch sliders for 360° rotational framing around focal targets.
  - FOV & Focal Length control (from 24° telephoto portrait to 105° ultra-wide action/fisheye).
  - Rate-limited asynchronous telemetry polling with auto-correction for login camera heights.
  - **One-Click Framing Presets**:
    - *Front Views*: Front Portrait (`front_portrait`), Front Medium (`front_medium`), Front Full Body (`front_full_body`), 3/4 Left Angle, 3/4 Right Angle.
    - *Rear Views*: Rear 3rd-Person, Over-the-Shoulder, Wide Landscape, Bird's-Eye Overhead.
- **Persistent Location Bookmarks (`saved_locations.json`)**:
  - Save current avatar XYZ, camera eye XYZ, camera target XYZ, and FOV with a custom name and notes.
  - One-click restore options: *Teleport Avatar Only*, *Reposition Camera Only*, or *Restore Both*.

### 4. 🎬 Cinematic Camera Fly-Through Sequencer (`tab_camera.rs`)
Machinima dolly shot generator and keyframe path interpolator:
- **Multi-Curvature Interpolation**:
  - `Linear`: Constant velocity between waypoints.
  - `EaseInOutQuad`: Smooth acceleration and deceleration.
  - `CatmullRomSpline`: 4-point cubic Hermite spline ensuring smooth path curvature without sharp angular corners.
- **Timeline Transport**: Interactive scrubber, Play, Pause, Stop, Loop mode, and 0.5x / 1.0x / 2.0x playback speed.
- **Generators**: 360° Orbit Tour, Skyline Crane Down, and 60m Forward Fly-Through.
- **Automated Benchmarking**: Measures streaming framerates and frametime jitter along the path.
- **Persistence**: Sequences saved automatically to `camera_sequences.json`.

### 5. 🧱 Inworld Objects & Linksets Inspector & Prim Creation (`tab_objects.rs`)
Visual inspection and live manipulation of nearby primitives within draw distance using standard `LLObjects` pumps:
- **Rez Primitive Toolbar**: Rezzes 13 standard geometric primitives (Box/Cube, Sphere, Cylinder, Prism, Pyramid, Tetrahedron, Cone, Torus, Tube, Ring) at avatar coordinates with custom sizing (0.1–10.0m).
- **Hierarchical Tree Browser**: Groups objects into Linksets (root prim + indented children with link counts).
- **Search & Filters**: Distance slider (8–256m), object name, local ID, and UUID substring search.
- **One-Click Actions**:
  - *Focus Camera*: Orients camera at object center with dynamic distance offset.
  - *Teleport To*: Teleports avatar to object's bounding box coordinates.
  - *Highlight In-World*: Triggers `LLObjects.selectObject` to highlight prims in simulator.
  - *Duplicate (+1m Z)*: Replicates selected object with a 1m elevation offset.
  - *Delete Prim*: Derezzes selected object into the avatar's trash folder.
  - *Inspect LLSD*: Formatted JSON / LLSD viewer for physics shape, velocity, scale, and click action.

### 6. 📡 2D Spatial Mini-Radar & Minimap (`tab_radar.rs`)
Top-down tactical radar displaying region boundaries and nearby avatars:
- **Dual Display Modes**: *Region Map (256m coordinates)* and *Avatar-Centric (concentric range rings 16m–128m)*.
- **Avatar Blips**: Displays name, distance, and elevation delta (`▲` above / `▼` below).
- **Canvas Interactions**:
  - Single-click: Select target coordinates for walking or teleporting.
  - **Double-click**: Immediately walk towards clicked coordinate.
  - **Right-click**: Immediately aim camera look-at vector towards clicked coordinate.

### 7. 🔔 Dialog Notifications, Alerts & Group Requests (`tab_dashboard.rs`, `client.rs`)
Modal dialog handling and permission request management via `LLNotifications`:
- **Real-Time Polling**: Tracks active dialogs, teleport offers, group invites, and script permission queries without human GUI intervention.
- **Interactive Action Cards**:
  - Script Dialog buttons (numbered button matrices).
  - Accept, Decline, and Dismiss buttons.
  - URL link handling for external web interactions.
- **Automation Bindings**: Full programmatic dispatch via Rhai (`notifications_get()`, `notification_respond()`) and MCP tools (`aurora_get_notifications`, `aurora_respond_notification`).

### 8. 📊 Simulator Health & Sparkline Profiler (`tab_dashboard.rs`)
Real-time diagnostic telemetry monitoring OpenSim / Second Life region server health:
- **Rolling Sparklines (Last 60 Seconds)**:
  - Simulator FPS (Target: 45.0 FPS) & Physics FPS.
  - Frame time in milliseconds (Target: 22.2 ms).
  - Active Avatars & Script Events per Second.
- **Health Badges**: `SIM HEALTHY`, `DEGRADED PERFORMANCE`, `SEVERE SIM LAG`.
- **Automated Rhai Assertion**: `assert_sim_fps_min(35.0)` for stress test verification.

### 9. 📜 Blackbox Flight Recorder & Incident Bundle Exporter (`tab_logs.rs`, `client.rs`)
Zero-overhead diagnostic capture for intermittent desyncs, viewer drops, or crashes:
- **Circular Memory Buffer**: Holds the last 250 incoming and outgoing LEAP protocol frames with microsecond latency metrics.
- **"📦 Export Incident Bundle (.zip)"**: Generates a compressed zip archive containing:
  - `leap_traffic.jsonl`: Chronological raw protocol trace.
  - `viewer_output.log`: Subprocess stderr/stdout console lines.
  - `system_info.json`: OS details, architecture, PID, and cached telemetry.

### 10. 🤖 Automated Scripting, Linter & Macro Recorder (`tab_scripting.rs`)
Complete embedded Rhai scripting engine with native LEAP bindings:
- **Realtime Linter**: Parses scripts on every keystroke and highlights syntax errors with line/column pointers.
- **"● Record Macro" Toggle**: Automatically captures manual camera movements, button clicks, teleports, and chat into formatted Rhai scripts with real-time `sleep(ms)` delays.
- **Batch Test Runner**: Sequential batch runner executing all catalog scripts with assertion passes/failures and markdown reports.

### 11. 🧠 Model Context Protocol (MCP) Server for AI Agents
Headless JSON-RPC 2.0 stdio server for Antigravity and external AI agents:
- **CLI Flag**: `aurora-leap-gui.exe --mcp-stdio`
- **27 Tools**: Complete toolset covering connection, login, telemetry, movement, camera, snapshots, bookmarks, radar, dialog notifications, script evaluation, raw LLSD, and full primitive creation and manipulation (`aurora_create_object`, `aurora_delete_object`, `aurora_set_object_transform`, `aurora_select_object`, `aurora_deselect_object`, `aurora_set_object_name`, `aurora_duplicate_object`).
- **Multimodal AI Vision**: Supports `capture_snapshot: true` parameter on navigation actions to return PNG snapshots for instant visual analysis.

---

## Command Line Usage

### 1. Launch Interactive Desktop GUI
```powershell
aurora-leap-gui.exe
```

### 2. Launch Stdio MCP Server (for Antigravity or AI Agents)
```powershell
aurora-leap-gui.exe --mcp-stdio
```

### 3. Headless CI/CD Batch Regression Test Runner
```powershell
aurora-leap-gui.exe --headless --run-suite scripts --export-report test_report.md --exit-on-finish
```

---

## Antigravity MCP Server Configuration

To connect Antigravity directly to `aurora-leap-gui`, add the following to your MCP settings (`mcp_config.json`):

```json
{
  "mcpServers": {
    "aurora-viewer": {
      "command": "d:\\AI code projects\\antigravity\\aurora-viewer\\aurora-leap-gui\\target\\debug\\aurora-leap-gui.exe",
      "args": [
        "--mcp-stdio"
      ],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```
