import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

function notifications(language = "en") {
  let strings;
  const window = { CarrotTranslations: { register(_language, value) { strings = value.strings; } } };
  const context = vm.createContext({ window, getUIText(key, fallback, vars = {}) {
    return String(strings[key] ?? fallback).replace(/\{(\w+)\}/g, (_, key) => vars?.[key] ?? `{${key}}`);
  } });
  vm.runInContext(readFileSync(new URL(`../js/translations/${language}.js`, import.meta.url), "utf8"), context);
  const source = readFileSync(new URL("../src/features/tools/notifications.js", import.meta.url), "utf8")
    .replace('import { getToolCommandPreview, renderToolsMeta } from "./runtime.js";', "")
    .replace("  global.CarrotToolsNotifications = {", "  global.contract = { localizedResultSummary, friendlyEmptyOutput, formatGitUpdateSummary, makeJobEntry };\n  global.CarrotToolsNotifications = {");
  vm.runInContext(source, context);
  return window.contract;
}
const messages = {
  en: "Verified update staged. Reboot to apply.",
  ko: "검증된 업데이트가 준비되었습니다. 적용하려면 재부팅하세요.",
  zh: "已准备好经过验证的更新。重启后应用。",
};
function job(action, result = {}) {
  return { id: "owned", action, status: "done", result: { ok: true, staged: true, reboot_required: true, ...result } };
}
for (const [language, expected] of Object.entries(messages)) {
  test(`${language}: checkout and reset report staged activation in card and detail`, () => {
    const view = notifications(language);
    for (const [action, key] of [["git_checkout", "git_result_checkout_done"], ["git_reset", "git_result_reset_done"], ["git_reset_repo_checkout", "git_result_reset_repo_checkout_done"]]) {
      const value = job(action, { summary_key: key, summary_vars: { branch: "candidate", mode: "hard", target: "HEAD" }, out: "owned package verification" });
      assert.equal(view.localizedResultSummary(value), expected);
      assert.equal(view.friendlyEmptyOutput(value), expected);
      const entry = view.makeJobEntry(value, 0);
      assert.equal(entry.summary, expected);
      assert.ok(entry.text.includes(expected));
    }
  });
}
test("pull commit summary and same-SHA repair do not imply active source changed", () => {
  const view = notifications();
  for (const updated of [false, true]) {
    const summary = { updated, commit_count: 1, commits: [{ hash: "1234567", message: "owned source update" }], files_changed: 1, insertions: 2, deletions: 1 };
    const value = job("git_pull", { update_summary: summary });
    const formatted = view.formatGitUpdateSummary(value, summary);
    assert.ok(formatted.card.startsWith(messages.en));
    assert.equal(formatted.label, messages.en);
    assert.ok(view.makeJobEntry(value, 0).summary.startsWith(messages.en));
    if (updated) assert.ok(formatted.card.includes("owned source update"));
  }
});
test("native repair distinguishes requested reboot from pending manual reboot", () => {
  const view = notifications();
  assert.equal(view.makeJobEntry(job("rebuild_all", { reboot_required: false, rebooting: true }), 0).summary, "Verified package repair staged. Reboot requested.");
});
test("failed native staging never displays the legacy branch-changed success", () => {
  const view = notifications();
  const value = job("git_checkout", { ok: false, staged: false, reboot_required: false, summary_key: "git_result_checkout_done", summary_vars: { branch: "candidate" }, out: "matching release unavailable" });
  value.status = "failed";
  assert.equal(view.localizedResultSummary(value), "Update was not staged.");
  assert.equal(view.friendlyEmptyOutput(value), "Update was not staged.");
  assert.ok(!view.makeJobEntry(value, 0).text.includes("Branch changed"));
});
test("legacy results without native staging flags retain their original message", () => {
  const view = notifications();
  const value = { action: "git_checkout", status: "done", result: { ok: true, summary_key: "git_result_checkout_done", summary_vars: { branch: "legacy" } } };
  assert.equal(view.localizedResultSummary(value), "Branch changed: legacy");
});
