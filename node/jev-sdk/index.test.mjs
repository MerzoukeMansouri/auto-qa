// node --test node/jev-sdk/index.test.mjs (needs `npm install` in node/jev-sdk)
import assert from "node:assert/strict";
import test from "node:test";
import { actionSpace, firstUrl, pageTable, parseSnapshot, verifyCriteria } from "./index.mjs";

const SNAP = `### Page
- Page URL: https://demo.playwright.dev/todomvc/#/
### Snapshot
\`\`\`yaml
- generic [ref=e1] [box=365,0,550,355]:
  - link "Jump to content" [ref=e2] [box=-1,-1,1,1]
  - heading "todos" [level=1] [ref=e7] [box=10,10,200,50]
  - textbox "What needs to be done?" [active] [ref=f3e8] [box=10,80,500,40]: Buy milk
  - link "real \\"TodoMVC\\" app." [ref=e3] [cursor=pointer] [box=737,130,100,20]:
    - /url: https://todomvc.com/
  - checkbox "Toggle Todo" [ref=f3e20] [box=10,160,20,20]
  - checkbox "Toggle Todo" [ref=f3e21] [box=10,190,20,20] [checked]
  - generic "Suivant" [ref=e50] [cursor=pointer] [box=100,400,80,30]
  - listitem [ref=e30] [box=10,160,500,30]:
    - text: Buy milk
  - contentinfo [ref=e40] [box=0,2000,1280,100]:
    - link "Footer link" [ref=e41] [box=10,2010,100,20]
    - text: Created by someone
\`\`\``;

test("snapshot to Jev state and options", () => {
  const p = parseSnapshot(SNAP);
  assert.equal(p.url, "https://demo.playwright.dev/todomvc/#/");
  const byRef = Object.fromEntries(p.elements.map((e) => [e.ref, e]));
  assert.deepEqual(byRef.f3e8, { role: "textbox", name: "What needs to be done?", value: "Buy milk", ref: "f3e8", where: "visible", cursorPointer: false });
  assert.equal(byRef.e3.name, 'real "TodoMVC" app.');
  // A styled div with no ARIA role but a pointer cursor is a de-facto button
  // (e.g. a custom SSO "Next" control) — must not be silently unclickable.
  assert.equal(byRef.e50.cursorPointer, true);
  assert.equal(byRef.e2.where, "hidden");
  assert.equal(byRef.e41.where, "below");
  // [checked] is a bare annotation (true) or simply absent (false); both must
  // surface as a value, or Jev can never see a checkbox change state.
  assert.equal(byRef.f3e20.value, "unchecked");
  assert.equal(byRef.f3e21.value, "checked");

  // Visible text only; no refs, boxes, urls, or offscreen footer text.
  const table = pageTable(p);
  assert.match(table, /textbox "What needs to be done\?": Buy milk/);
  assert.match(table, /text: Buy milk/);
  assert.doesNotMatch(table, /ref=|box=|\/url|Footer|Created by/);

  const s = actionSpace(p.elements, false);
  // Operation head stays small: only ops with a candidate target are offered.
  assert.deepEqual(Object.keys(s.operation), ["WAIT", "BLOCKED", "CLICK", "TYPE"]);
  assert.ok(!("HOVER" in s.targets));
  // Hidden skip link dropped, offscreen kept last and labeled; the two
  // "Toggle Todo" checkboxes are NOT collapsed — their checked state differs.
  assert.deepEqual(Object.keys(s.targets.TYPE), ["f3e8"]);
  assert.deepEqual(Object.keys(s.targets.CLICK), ["e3", "f3e20", "f3e21", "e50", "e41"]);
  assert.match(s.targets.CLICK.f3e20, /current: unchecked/);
  assert.match(s.targets.CLICK.f3e21, /current: checked/);
  assert.match(s.targets.CLICK.e41, /offscreen, below/);
  assert.match(s.operation.WAIT, /Example:/);
  assert.ok("DONE" in actionSpace(p.elements, true).operation);
  assert.ok(!("WAIT" in actionSpace(p.elements, true, new Set(), false).operation));
  // An action that already failed is not offered again.
  const failedDesc = `CLICK ${s.targets.CLICK.e3}`;
  assert.ok(!("e3" in actionSpace(p.elements, false, new Set([failedDesc])).targets.CLICK));

  const v = verifyCriteria(p.elements);
  // Different checked state means different proof text, so both checkboxes stay distinct options.
  assert.ok("VERIFY:e7" in v && !("VERIFY:e41" in v));
  assert.match(v["VERIFY:f3e21"], /: checked$/);
});

test("options stay under Jev's 255 cap", () => {
  const els = Array.from({ length: 1000 }, (_, i) => ({ role: "link", name: `L${i}`, value: "", ref: `e${i}`, where: "visible" }));
  const s = actionSpace(els, true);
  assert.ok(Object.keys(s.operation).length <= 255);
  assert.ok(Object.keys(s.targets.CLICK).length <= 255);
  assert.ok(Object.keys(verifyCriteria(els)).length <= 255);
});

test("start URL from query", () => {
  assert.equal(firstUrl("go to https://demo.playwright.dev/todomvc/, verify x"), "https://demo.playwright.dev/todomvc/");
  assert.equal(firstUrl("sur le site leroymerlin.fr trouve moi le marteau"), "https://leroymerlin.fr");
  assert.equal(firstUrl("va sur www.leroymerlin.fr/outillage."), "https://www.leroymerlin.fr/outillage");
  assert.equal(firstUrl("trouve le marteau, e.g. le moins cher"), null);
});
