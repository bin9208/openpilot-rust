// Receives the actual populated /api/live_runtime response from the IPC/HTTP probe.
import assert from "node:assert/strict";
import { normalizeDriveInsightsSnapshot } from "../../openpilot/selfdrive/carrot/web/src/features/drive/contents/drive_insights/schema.js";

let input = "";
for await (const chunk of process.stdin) input += chunk;
const response = JSON.parse(input);
const snapshot = normalizeDriveInsightsSnapshot({
  timestampMs: 1000,
  hudState: response.services,
  overlayState: {},
});
assert.equal(snapshot.navigation.distanceM, 35.5);
assert.equal(snapshot.navigation.maneuverType, "turn");
assert.equal(response.services.navInstructionCarrot.mainText, "owned populated navigation");
assert.equal(response.services.navInstructionCarrot.distanceText, "36m");
console.log("populated native navigation reaches browser in meters PASS");
