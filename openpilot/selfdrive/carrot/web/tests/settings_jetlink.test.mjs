import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const schema = JSON.parse(readFileSync(new URL("../../../carrot_settings.json", import.meta.url), "utf8"));
const item = schema.params.find((item) => item.name === "JetlinkMode");
const source = readFileSync(new URL("../js/pages/setting.js", import.meta.url), "utf8");

function functionSource(name) {
  const start = source.search(new RegExp(`^function ${name}\\(`, "m"));
  assert.notEqual(start, -1);
  return source.slice(start, source.indexOf("\n}", start) + 2);
}

function controls(language = "en") {
  const context = vm.createContext({
    LANG: language,
    SETTING_CONTROL_KINDS: ["toggle", "segmented", "select", "slider"],
    findSettingItemByName: () => ({ item }),
    getSettingDisplayUnit: () => "",
    window: {},
  });
  for (const name of ["getSettingControlConfig", "getDeclaredSettingOptionLabel", "formatSettingDisplayValue", "getSettingOptionValues", "getSettingOptionLabel", "syncSettingControlState"]) {
    vm.runInContext(functionSource(name), context);
  }
  return context;
}

test("Jetlink exposes exactly Off and actual external output, with Off as default", () => {
  assert.deepEqual([item.min, item.max, item.default], [0, 1, 0]);
  const context = controls();
  const config = context.getSettingControlConfig(item);
  assert.equal(config.kind, "segmented");
  assert.deepEqual(Array.from(context.getSettingOptionValues(item.name, config)), [0, 1]);
  for (const language of ["ko", "en", "zh"]) {
    assert.equal(item.options[language].length, 2);
  }
  assert.deepEqual(item.options.en, ["Off (native model)", "External GPU model"]);
});

test("saved 1 and legacy 2 display the same selected external choice without writing settings", () => {
  for (const value of [0, 1, 2, "2"]) {
    const context = controls();
    const expected = Number(value) === 2 ? 1 : Number(value);
    const valueButton = { dataset: {} };
    const segments = [0, 1].map((value) => ({
      dataset: { value: String(value) },
      classList: { toggle(_name, active) { this.active = active; } },
      setAttribute(name, value) { this[name] = value; },
    }));
    context.syncSettingControlState({
      dataset: { settingName: "JetlinkMode" },
      querySelector: (selector) => selector === ".val" ? valueButton : null,
      querySelectorAll: () => segments,
    }, value);
    assert.equal(valueButton.textContent, item.options.en[expected]);
    assert.equal(valueButton.dataset.rawValue, String(expected));
    assert.equal(segments[expected]["aria-pressed"], "true");
    assert.equal(segments[1 - expected]["aria-pressed"], "false");
  }
});

test("legacy display migration leaves unrelated settings and unknown values alone", () => {
  const context = controls();
  assert.equal(context.formatSettingDisplayValue({ name: "JetlinkMode" }, 3), "3");
  // An unrelated setting still indexes its own third option, not the Jetlink alias.
  item.options.en.push("Third choice");
  try {
    assert.equal(context.formatSettingDisplayValue({ name: "OtherSetting" }, 2), "Third choice");
  } finally {
    item.options.en.pop();
  }
});
