//! Aurora-Viewer LEAP Studio
//!
//! A modern, interactive graphical application written in Rust using `serde-llsd`
//! and `eframe`/`egui` to interact with and test all Second Life LEAP features in Aurora-Viewer.

mod codec;
mod client;
pub mod landmarks;
pub mod mcp;
pub mod scripting;
mod tests;
mod ui;

use ui::LeapStudioApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();

    // ── CLI Mode 1: MCP JSON-RPC 2.0 Stdio Server ───────────────────────────────
    if args.iter().any(|a| a == "--mcp-stdio" || a == "--mcp") {
        mcp::run_mcp_stdio_server()?;
        return Ok(());
    }

    // ── CLI Mode 2: Headless CI/CD Automated Test Suite Runner ─────────────────
    if args.iter().any(|a| a == "--headless" || a == "--run-suite") {
        run_headless_suite(&args)?;
        return Ok(());
    }

    // ── GUI Mode: eframe Interactive Studio App ────────────────────────────────
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1150.0, 800.0])
            .with_min_inner_size([900.0, 650.0])
            .with_title("Aurora-Viewer LEAP Studio")
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "Aurora-Viewer LEAP Studio",
        native_options,
        Box::new(|cc| {
            // Configure clean dark visuals
            let mut visuals = egui::Visuals::dark();
            visuals.override_text_color = Some(egui::Color32::from_rgb(220, 225, 235));
            visuals.window_fill = ui::theme::COLOR_BG_DARK;
            visuals.panel_fill = ui::theme::COLOR_BG_DARK;
            cc.egui_ctx.set_visuals(visuals);

            Ok(Box::new(LeapStudioApp::new(cc)))
        }),
    )
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    Ok(())
}

/// Execute all catalog test scripts headlessly and return pass/fail exit code.
fn run_headless_suite(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Aurora-Viewer LEAP Headless CI Runner ===");
    let mut catalog = scripting::catalog::ScriptCatalog::new();
    catalog.refresh();

    if catalog.scripts.is_empty() {
        eprintln!("No .rhai scripts found in {}", catalog.dir_path.display());
        std::process::exit(1);
    }

    let mut client = client::LeapClient::new();
    let viewer_arg = args.windows(2).find(|w| w[0] == "--viewer-path").map(|w| w[1].as_str());

    let candidate_paths = [
        "Aurora-Viewer/target/debug/aurora-viewer.exe",
        "../Aurora-Viewer/target/debug/aurora-viewer.exe",
        "target/debug/aurora-viewer.exe",
        "aurora-viewer.exe",
    ];
    let mut exe_to_spawn = viewer_arg.map(std::path::PathBuf::from);
    if exe_to_spawn.is_none() {
        for c in candidate_paths {
            let p = std::path::PathBuf::from(c);
            if p.exists() {
                exe_to_spawn = Some(p);
                break;
            }
        }
    }

    if let Some(exe) = exe_to_spawn {
        println!("[CI] Spawning viewer at '{}'...", exe.display());
        match client.spawn_and_connect(&exe, "") {
            Ok(_) => println!("[CI] Connected to viewer process (PID: {:?})", client.child_pid()),
            Err(e) => eprintln!("[CI] Note: Could not spawn viewer ({e}). Running scripts in offline mode."),
        }
    }

    let export_path = args.windows(2).find(|w| w[0] == "--export-report").map(|w| w[1].as_str());

    let mut passed_count = 0;
    let mut failed_count = 0;
    let mut report_rows = Vec::new();
    let total_start = std::time::Instant::now();

    for script_name in &catalog.scripts {
        print!("Running `{script_name}`... ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let code = match catalog.load_script(script_name) {
            Some(c) => c,
            None => {
                println!("FAIL (Could not load script)");
                failed_count += 1;
                continue;
            }
        };

        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ctx = scripting::engine::ScriptContext::new(client.clone(), event_tx, cancel_flag);
        let engine = scripting::engine::create_script_engine(ctx);

        let mut scope = rhai::Scope::new();
        let t0 = std::time::Instant::now();
        let eval_res = engine.run_with_scope(&mut scope, &code);
        let elapsed_ms = t0.elapsed().as_millis() as u64;

        let mut assertions_passed = 0;
        let mut assertions_failed = 0;
        while let Ok(ev) = event_rx.try_recv() {
            if let scripting::engine::ScriptEvent::Assertion(ast) = ev {
                if ast.passed {
                    assertions_passed += 1;
                } else {
                    assertions_failed += 1;
                }
            }
        }

        let is_ok = eval_res.is_ok() && assertions_failed == 0;
        if is_ok {
            println!("PASS ({} passed, {} failed, {}ms)", assertions_passed, assertions_failed, elapsed_ms);
            passed_count += 1;
        } else {
            let err_msg = eval_res.err().map(|e| e.to_string()).unwrap_or_else(|| format!("{assertions_failed} assertions failed"));
            println!("FAIL ({}ms): {err_msg}", elapsed_ms);
            failed_count += 1;
        }

        report_rows.push((script_name.clone(), is_ok, assertions_passed, assertions_failed, elapsed_ms));
    }

    let total_elapsed = total_start.elapsed().as_millis() as u64;
    println!("\n=== Test Suite Summary ===");
    println!("Total Scripts: {}", catalog.scripts.len());
    println!("Passed: {passed_count}");
    println!("Failed: {failed_count}");
    println!("Total Time: {total_elapsed} ms");

    if let Some(out_path) = export_path {
        let mut md = String::new();
        md.push_str("# Aurora-Viewer LEAP CI Test Suite Report\n\n");
        md.push_str(&format!("- **Timestamp**: {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        md.push_str(&format!("- **Total**: {}\n", catalog.scripts.len()));
        md.push_str(&format!("- **Passed**: {passed_count}\n"));
        md.push_str(&format!("- **Failed**: {failed_count}\n"));
        md.push_str(&format!("- **Duration**: {total_elapsed} ms\n\n"));
        md.push_str("| Script | Result | Passed | Failed | Duration |\n| :--- | :---: | :---: | :---: | :---: |\n");
        for (name, ok, p, f, dur) in &report_rows {
            let tag = if *ok { "✅ PASS" } else { "❌ FAIL" };
            md.push_str(&format!("| `{name}` | {tag} | {p} | {f} | {dur} ms |\n"));
        }
        std::fs::write(out_path, md)?;
        println!("[CI] Report exported to: {out_path}");
    }

    if failed_count > 0 {
        std::process::exit(1);
    }
    Ok(())
}
