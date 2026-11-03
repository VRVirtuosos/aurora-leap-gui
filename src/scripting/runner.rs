//! Background script runner and lifecycle coordinator.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use chrono::Local;
use crossbeam_channel::{unbounded, Receiver, Sender};
use rhai::{Dynamic, Scope};

use crate::client::LeapClient;
use crate::scripting::engine::{create_script_engine, ScriptContext, ScriptEvent};
use crate::scripting::models::{LogLevel, ScriptLogMessage, ScriptStatus, TestReport};

pub struct ScriptRunner {
    pub status: ScriptStatus,
    pub current_script_name: String,
    pub current_loop: usize,
    pub total_loops: usize,
    pub cancel_flag: Arc<AtomicBool>,
    pub report: TestReport,
    event_tx: Sender<ScriptEvent>,
    event_rx: Receiver<ScriptEvent>,
    worker_handle: Option<JoinHandle<()>>,
}

impl ScriptRunner {
    pub fn new() -> Self {
        let (event_tx, event_rx) = unbounded();
        Self {
            status: ScriptStatus::Idle,
            current_script_name: String::new(),
            current_loop: 0,
            total_loops: 1,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            report: TestReport::default(),
            event_tx,
            event_rx,
            worker_handle: None,
        }
    }

    /// Poll incoming events from the worker thread and update internal status.
    pub fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                ScriptEvent::Started { script_name } => {
                    self.status = ScriptStatus::Running;
                    self.current_script_name = script_name.clone();
                    self.report.script_name = script_name;
                    self.report.start_time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                }
                ScriptEvent::Log(msg) => {
                    self.report.logs.push(msg);
                }
                ScriptEvent::Assertion(rec) => {
                    self.report.total_assertions += 1;
                    if rec.passed {
                        self.report.passed_assertions += 1;
                    } else {
                        self.report.failed_assertions += 1;
                    }
                    self.report.assertions.push(rec);
                }
                ScriptEvent::Finished { duration_ms, .. } => {
                    self.report.duration_ms = duration_ms;
                    if self.report.failed_assertions > 0 {
                        self.status = ScriptStatus::Failed;
                    } else {
                        self.status = ScriptStatus::Succeeded;
                    }
                }
                ScriptEvent::Error { message } => {
                    self.status = ScriptStatus::Failed;
                    self.report.logs.push(ScriptLogMessage::new(LogLevel::Fail, format!("SCRIPT RUNTIME ERROR: {message}")));
                }
            }
        }
    }

    /// Launch script execution on the background thread.
    pub fn start(&mut self, script_name: String, code: String, loops: usize, client: LeapClient) {
        if self.status == ScriptStatus::Running {
            return;
        }

        self.cancel_flag.store(false, Ordering::Relaxed);
        self.status = ScriptStatus::Running;
        self.current_script_name = script_name.clone();
        self.total_loops = loops.max(1);
        self.current_loop = 1;

        // Reset report
        self.report = TestReport {
            script_name: script_name.clone(),
            start_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            duration_ms: 0,
            total_assertions: 0,
            passed_assertions: 0,
            failed_assertions: 0,
            assertions: Vec::new(),
            logs: Vec::new(),
        };

        let cancel_flag = self.cancel_flag.clone();
        let event_tx = self.event_tx.clone();

        self.worker_handle = Some(thread::spawn(move || {
            let start_time = Instant::now();
            let _ = event_tx.send(ScriptEvent::Started { script_name: script_name.clone() });

            let ctx = ScriptContext::new(client, event_tx.clone(), cancel_flag.clone());
            let engine = create_script_engine(ctx.clone());

            for loop_idx in 1..=loops {
                if cancel_flag.load(Ordering::Relaxed) {
                    let _ = event_tx.send(ScriptEvent::Log(ScriptLogMessage::new(
                        LogLevel::Warn,
                        format!("Execution cancelled by user during loop {loop_idx}/{loops}."),
                    )));
                    break;
                }

                if loops > 1 {
                    let _ = event_tx.send(ScriptEvent::Log(ScriptLogMessage::new(
                        LogLevel::Info,
                        format!("--- Starting Loop {loop_idx}/{loops} ---"),
                    )));
                }

                let mut scope = Scope::new();
                match engine.eval_with_scope::<Dynamic>(&mut scope, &code) {
                    Ok(_) => {}
                    Err(e) => {
                        let _ = event_tx.send(ScriptEvent::Error {
                            message: format!("{e}"),
                        });
                        break;
                    }
                }
            }

            let duration_ms = start_time.elapsed().as_millis() as u64;
            let assertions = ctx.assertions.lock().unwrap();
            let total = assertions.len();
            let passed = assertions.iter().filter(|a| a.passed).count();
            let failed = total - passed;

            let _ = event_tx.send(ScriptEvent::Finished {
                total_assertions: total,
                passed,
                failed,
                duration_ms,
            });
        }));
    }

    /// Abort execution.
    pub fn stop(&mut self) {
        if self.status == ScriptStatus::Running {
            self.cancel_flag.store(true, Ordering::Relaxed);
            self.status = ScriptStatus::Stopped;
            let _ = self.event_tx.send(ScriptEvent::Log(ScriptLogMessage::new(
                LogLevel::Warn,
                "Script execution cancellation signaled.",
            )));
        }
    }

    /// Clear console and report.
    pub fn clear(&mut self) {
        self.report = TestReport::default();
        if self.status != ScriptStatus::Running {
            self.status = ScriptStatus::Idle;
        }
    }
}
