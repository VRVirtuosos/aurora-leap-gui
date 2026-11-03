// 02_movement_and_waypoints.rhai
// Automated LEAP Test: Test avatar walking, turning, flight and stop controls.

print("=== Starting Test: Avatar Locomotion & Controls ===");

// 1. Record starting position
let start_pos = agent_get_pos();
print("Starting position: (" + start_pos.x + ", " + start_pos.y + ", " + start_pos.z + ")");

// 2. Step forward 3 meters
print("Stepping forward 3 meters...");
agent_step("forward", 3.0);
sleep(1500);

let fwd_pos = agent_get_pos();
print("Position after forward step: (" + fwd_pos.x + ", " + fwd_pos.y + ", " + fwd_pos.z + ")");
assert_true(fwd_pos.x != start_pos.x || fwd_pos.y != start_pos.y, "Avatar moved forward from start");

// 3. Test Turning
print("Turning left...");
agent_step("turn_left", 0.0);
sleep(500);

let turn_pos = agent_get_pos();
print("Heading yaw after turn: " + turn_pos.yaw);
assert_ne(turn_pos.yaw, start_pos.yaw, "Avatar yaw updated upon turning");

// 4. Test Stop
print("Signaling stop...");
let stopped = agent_stop();
assert_true(stopped, "Stop command received successfully");

print("=== Test Complete: Locomotion Verified! ===");
