// 03_snapshot_regression.rhai
// Automated LEAP Test: Capture inworld snapshots and verify frame buffer capture.

print("=== Starting Test: Viewport Snapshot Capture ===");

// 1. Capture native window viewport snapshot
print("Capturing native window viewport snapshot...");
let native_path = snapshot_capture("native", 0, 0);
assert_true(native_path != "", "Native window snapshot file path was returned");
print("Saved native snapshot to: " + native_path);

// 2. Capture HD 1280x720 snapshot
print("Capturing 1280x720 HD snapshot...");
let hd_path = snapshot_capture("hd", 1280, 720);
assert_true(hd_path != "", "HD snapshot file path was returned");
print("Saved HD snapshot to: " + hd_path);

print("=== Test Complete: Snapshots Captured Successfully! ===");
