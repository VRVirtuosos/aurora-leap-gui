// 01_login_and_verify.rhai
// Automated LEAP Test: Verify viewer authentication and inworld status.

print("=== Starting Test: Viewer Login Verification ===");

// 1. Query current viewer status
let state = login_get_state();
print("Current viewer state: " + state);

// 2. Assert whether viewer is connected or waiting
if state == "STATE_STARTED" {
    print("Avatar is already active inworld!");
    assert_true(true, "Viewer is in STATE_STARTED");
} else {
    print("Waiting up to 10 seconds for viewer to reach inworld status...");
    let logged_in = login_wait_until_inworld(10);
    assert_true(logged_in, "Viewer reached inworld state (STATE_STARTED)");
}

// 3. Inspect avatar telemetry
let pos = agent_get_pos();
print("Region: " + pos.region);
print("Position: X=" + pos.x + ", Y=" + pos.y + ", Z=" + pos.z);

assert_true(pos.region != "", "Region name is populated");
assert_true(pos.x > 0.0, "Avatar X coordinate is positive");
assert_true(pos.y > 0.0, "Avatar Y coordinate is positive");

print("=== Test Complete: Avatar Login Verified! ===");
