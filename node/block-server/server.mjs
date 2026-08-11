// autoqa's own MCP server, run as a sibling to Playwright MCP in the same
// harness invocation. Exposes `list_blocks` (so the agent can discover what
// slugs/placeholders exist) and `run_block` (deterministic replay of a
// block's literal Playwright JS against the *same* browser session
// Playwright MCP is driving — both attach to the CDP endpoint autoqa itself
// launched Chrome with, so there is no separate/competing browser).
import { appendFile, readFile } from "node:fs/promises";
import path from "node:path";
import { chromium, expect } from "@playwright/test";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

function argValue(flag) {
  const i = process.argv.indexOf(flag);
  if (i === -1 || i + 1 >= process.argv.length) {
    throw new Error(`missing required arg ${flag}`);
  }
  return process.argv[i + 1];
}

const cdpEndpoint = argValue("--cdp-endpoint");
const blocksDir = argValue("--blocks-dir");
const paramsFile = argValue("--params-file");
const runLogFile = argValue("--run-log");
const pwSessionDir = argValue("--pw-session-dir");
// The newest session-* directory name that already existed before this run
// started (empty string if none did) — anything at or before this belongs
// to a past, unrelated run and must never be mistaken for "the current
// session" (see the comment on `currentSessionBytes` below).
const pwSessionBaseline = argValue("--pw-session-baseline");

// Playwright MCP's own `--save-session` recording only sees *its own* tool
// calls — a sibling MCP server like this one is invisible to it, and most
// of its recorded entries carry no usable timestamp of their own (only the
// rare one with a console-log `events` reference does). So instead of a
// clock, this reads Playwright's session.md file size *right now* — a
// precise "this many bytes existed when run_block was called" boundary the
// Rust side (src/playwright_codegen.rs's `parse_mcp_session_with_offset`)
// can merge against exactly, no clock skew or missing-timestamp gaps.
// `--output-dir` names the *parent* of Playwright MCP's own session-<ms>
// subdir (it picks that name itself, lazily, on ITS OWN first tool call —
// which may well be *after* this run_block call, e.g. when a TUI-planned
// block is meant to replay before anything else). Naively picking "the
// newest directory on disk" would then find a leftover directory from some
// past, unrelated run instead — only directories strictly newer than
// `pwSessionBaseline` can possibly belong to this run; if none exist yet,
// Playwright hasn't started writing anything, so 0 bytes ("before
// everything") is the correct answer, not a fallback to stale data.
async function currentSessionBytes() {
  const { readdir, stat } = await import("node:fs/promises");
  let dirs;
  try {
    dirs = (await readdir(pwSessionDir, { withFileTypes: true }))
      .filter((d) => d.isDirectory())
      .map((d) => d.name)
      .filter((name) => name > pwSessionBaseline)
      .sort();
  } catch (err) {
    if (err.code === "ENOENT") return 0;
    throw err;
  }
  const latest = dirs.at(-1);
  if (!latest) return 0;
  try {
    return (await stat(path.join(pwSessionDir, latest, "session.md"))).size;
  } catch (err) {
    if (err.code === "ENOENT") return 0;
    throw err;
  }
}

async function logRunBlock(slug, bindings) {
  const line = JSON.stringify({
    sessionBytes: await currentSessionBytes(),
    slug,
    bindings: bindings ?? {},
  });
  await appendFile(runLogFile, line + "\n");
}

async function readJson(filePath, fallback) {
  try {
    return JSON.parse(await readFile(filePath, "utf8"));
  } catch (err) {
    if (err.code === "ENOENT" && fallback !== undefined) return fallback;
    throw err;
  }
}

async function readBlock(slug) {
  const file = path.join(blocksDir, `${slug}.json`);
  try {
    return await readJson(file);
  } catch (err) {
    if (err.code === "ENOENT") {
      throw new Error(`block '${slug}' not found at ${file}`);
    }
    throw err;
  }
}

// Mirrors src/playwright_codegen.rs's `resolve_placeholders` — a `{{name}}`
// token in a block's action/assertion string is replaced with the *value*
// of whichever param `bindings[name]` points to. Both a missing binding and
// a binding pointing at a param name that doesn't exist are hard errors,
// matching the Rust-side codegen behavior (no silently-unsubstituted code
// reaches `page`/`expect`).
async function resolvePlaceholders(text, slug, bindings) {
  const params = await readJson(paramsFile, []);
  const tokenPattern = /\{\{([^}]+)\}\}/g;
  let result = "";
  let lastIndex = 0;
  for (const match of text.matchAll(tokenPattern)) {
    const placeholder = match[1];
    const paramName = bindings[placeholder];
    if (paramName === undefined) {
      throw new Error(
        `block '${slug}': placeholder '{{${placeholder}}}' has no binding`,
      );
    }
    const param = params.find((p) => p.name === paramName);
    if (param === undefined) {
      throw new Error(
        `block '${slug}': placeholder '{{${placeholder}}}' bound to param '${paramName}', which doesn't exist`,
      );
    }
    result += text.slice(lastIndex, match.index) + param.value;
    lastIndex = match.index + match[0].length;
  }
  return result + text.slice(lastIndex);
}

// Playwright MCP drives a single page in a single browser context — reuse
// that same page rather than opening a new one, so the block's steps see
// the same DOM/navigation state the agent has already built up.
async function attachToPage() {
  const browser = await chromium.connectOverCDP(cdpEndpoint);
  const context = browser.contexts()[0];
  if (!context) throw new Error("no browser context open yet — has the agent navigated anywhere?");
  const page = context.pages()[context.pages().length - 1];
  if (!page) throw new Error("browser context has no open page");
  return { browser, page };
}

// Steps are stored as raw Playwright JS statement strings (same convention
// as ActionEntry/TestStep on the Rust side) — replay is a literal eval
// against `page`/`expect`, not a re-interpretation of intent.
async function runStatement(page, statement) {
  const fn = new Function(
    "page",
    "expect",
    `return (async () => { ${statement} })();`,
  );
  await fn(page, expect);
}

const server = new McpServer({ name: "autoqa-blocks", version: "0.1.0" });

server.registerTool(
  "list_blocks",
  {
    description:
      "List reusable step blocks available to replay via run_block, with their slug and any {{placeholder}} names that need a binding.",
    inputSchema: {},
  },
  async () => {
    const { readdir } = await import("node:fs/promises");
    let files = [];
    try {
      files = (await readdir(blocksDir)).filter((f) => f.endsWith(".json"));
    } catch (err) {
      if (err.code !== "ENOENT") throw err;
    }
    const blocks = await Promise.all(
      files.map(async (f) => {
        const slug = f.replace(/\.json$/, "");
        const block = await readBlock(slug);
        const placeholders = new Set();
        for (const step of block.steps) {
          for (const text of [step.action, step.assertion]) {
            for (const m of (text ?? "").matchAll(/\{\{([^}]+)\}\}/g)) {
              placeholders.add(m[1]);
            }
          }
        }
        return { slug, name: block.name, placeholders: [...placeholders] };
      }),
    );
    return { content: [{ type: "text", text: JSON.stringify(blocks) }] };
  },
);

server.registerTool(
  "run_block",
  {
    description:
      "Replay a saved block's steps verbatim against the live browser session — deterministic, no re-derivation of selectors. Call list_blocks first to see available slugs and required placeholder bindings.",
    inputSchema: {
      slug: z.string(),
      bindings: z.record(z.string()).optional(),
    },
  },
  async ({ slug, bindings }) => {
    const block = await readBlock(slug);
    // Logged before replay starts, and even if it later throws — a partial
    // replay still mutated the real page, so the block's position in the
    // reconstructed step list should reflect that regardless of outcome.
    await logRunBlock(slug, bindings);
    const { browser, page } = await attachToPage();
    try {
      for (const step of block.steps) {
        for (const raw of [step.action, step.assertion]) {
          if (!raw) continue;
          const resolved = await resolvePlaceholders(raw, slug, bindings ?? {});
          await runStatement(page, resolved);
        }
      }
      return {
        content: [
          { type: "text", text: `replayed block '${slug}' (${block.steps.length} steps)` },
        ],
      };
    } finally {
      // For a CDP-attached Browser, .close() disconnects this client without
      // killing the underlying Chrome process (autoqa/Rust owns that
      // lifecycle) — unverified against a live run, double-check this
      // doesn't tear down the agent's session before relying on it.
      await browser.close();
    }
  },
);

await server.connect(new StdioServerTransport());
