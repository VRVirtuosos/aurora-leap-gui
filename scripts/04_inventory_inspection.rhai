// 04_inventory_inspection.rhai
// Automated LEAP Test: Inspect inventory root and verify item queries.

print("=== Starting Test: Inventory Subsystem Inspection ===");

// 1. Query root inventory folder
print("Querying inventory root...");
let root = inventory_get_root();
assert_true(root.contains("folder_id"), "Root inventory contains folder_id");
print("Root Folder UUID: " + root.folder_id);

// 2. Test chat broadcast
print("Broadcasting test chat message...");
let chat_ok = chat_say("Automated LEAP test suite running successfully", 0);
assert_true(chat_ok, "Local chat dispatch acknowledged");

print("=== Test Complete: Inventory & Chat Verified! ===");
