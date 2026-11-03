// 05_camera_and_views.rhai: Automated Camera Control and Viewpoint Test
print("=== Starting Test: Camera Optics, Navigation & Bookmarking ===");

// 1. Query live camera telemetry
let cam = camera_get();
print("Camera Position: X=" + cam.x + ", Y=" + cam.y + ", Z=" + cam.z);
print("Camera Target: X=" + cam.target_x + ", Y=" + cam.target_y + ", Z=" + cam.target_z);
print("Field of View: " + cam.fov + "°");

assert_true(cam.fov > 10.0, "Camera FOV is initialized");

// 2. Test direct camera transform set
print("Applying test camera position and focal target...");
let set_ok = camera_set(122.0, 126.0, 27.5, 128.0, 128.0, 26.0, 55.0);
assert_true(set_ok, "camera_set successfully dispatched");

sleep(0.3);

// 3. Test relative camera pan navigation
print("Panning camera 2.5m forward, 1.0m elevation...");
let pan_ok = camera_pan(2.5, 0.0, 1.0);
assert_true(pan_ok, "camera_pan executed successfully");

// 4. Test camera orbit around target
print("Orbiting camera 20° yaw around target focus point...");
let orbit_ok = camera_orbit(20.0, 5.0);
assert_true(orbit_ok, "camera_orbit executed successfully");

// 5. Test optics FOV lens changes
print("Switching lens to 35° telephoto portrait...");
let fov_ok = camera_set_fov(35.0);
assert_true(fov_ok, "camera_set_fov executed successfully");

// 6. Test location bookmarking
print("Saving current camera and avatar viewpoint to bookmarks...");
let bookmark_ok = location_bookmark("Automated Script View");
assert_true(bookmark_ok, "location_bookmark saved successfully");

// 7. Reset camera back to default view
print("Resetting camera view to viewer default...");
let reset_ok = camera_reset();
assert_true(reset_ok, "camera_reset executed successfully");

print("=== Test Complete: Camera Navigation & Optics Verified! ===");
