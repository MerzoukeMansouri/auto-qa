#!/usr/bin/env node
// Own agent loop against the Claude Messages API directly (no claude-code
// CLI subprocess, and no Claude Agent SDK either — that SDK bundles and
// spawns the claude-code binary internally, same dependency this harness
// exists to avoid). Connects to whatever MCP servers are listed in
// --mcp-config-file, turns their tools into Anthropic tool definitions, and
// drives the call-model -> run-tool -> feed-result-back loop by hand.
import Anthropic from "@anthropic-ai/sdk";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import fs from "node:fs";

const DEFAULT_MODEL = "claude-haiku-4-5";
const MAX_TOKENS = 8192;

function parseArgs(argv) {
  const args = {
    query: null,
    systemPromptFile: null,
    mcpConfigFile: null,
    maxIterations: 50,
    raw: false,
    model: DEFAULT_MODEL,
  };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--system-prompt-file") args.systemPromptFile = argv[++i];
    else if (a === "--mcp-config-file") args.mcpConfigFile = argv[++i];
    else if (a === "--max-iterations") args.maxIterations = parseInt(argv[++i], 10);
    else if (a === "--raw") args.raw = true;
    else if (a === "--model") args.model = argv[++i];
    else rest.push(a);
  }
  args.query = rest[0];
  return args;
}

function truncate(v, n = 300) {
  const s = typeof v === "string" ? v : JSON.stringify(v);
  return s.length > n ? s.slice(0, n) + "…" : s;
}

// Tool names are namespaced mcp__<server>__<tool> to match the mcp__ prefix
// convention the shared system prompt already writes tool references in
// (see SYSTEM_PROMPT in src/agent.rs).
async function connectMcpServers(mcpConfigFile) {
  if (!mcpConfigFile || !fs.existsSync(mcpConfigFile)) {
    return { clients: [], tools: [], toolToClient: new Map() };
  }
  const config = JSON.parse(fs.readFileSync(mcpConfigFile, "utf8"));
  const servers = config.mcpServers ?? {};
  const clients = [];
  const tools = [];
  const toolToClient = new Map();
  for (const [name, spec] of Object.entries(servers)) {
    const transport = new StdioClientTransport({ command: spec.command, args: spec.args ?? [] });
    const client = new Client({ name: `autoqa-claude-sdk-${name}`, version: "1.0.0" });
    await client.connect(transport);
    const { tools: serverTools } = await client.listTools();
    for (const t of serverTools) {
      const qualifiedName = `mcp__${name}__${t.name}`;
      tools.push({
        name: qualifiedName,
        description: t.description ?? "",
        input_schema: t.inputSchema ?? { type: "object", properties: {} },
      });
      toolToClient.set(qualifiedName, { client, originalName: t.name });
    }
    clients.push(client);
  }
  return { clients, tools, toolToClient };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.query) {
    console.error("missing query argument");
    process.exit(1);
  }
  const apiKey = process.env.ANTHROPIC_API_KEY;
  if (!apiKey) {
    console.error("ANTHROPIC_API_KEY not set");
    process.exit(1);
  }
  const system = args.systemPromptFile ? fs.readFileSync(args.systemPromptFile, "utf8") : undefined;

  const { clients, tools, toolToClient } = await connectMcpServers(args.mcpConfigFile);
  const client = new Anthropic({ apiKey });

  const messages = [{ role: "user", content: args.query }];
  let finalText = "";

  try {
    for (let iter = 0; iter < args.maxIterations; iter++) {
      const stream = client.messages.stream({
        model: args.model,
        max_tokens: MAX_TOKENS,
        system,
        messages,
        ...(tools.length ? { tools } : {}),
      });
      if (!args.raw) {
        stream.on("text", (delta) => process.stdout.write("💬 " + delta));
      }
      const final = await stream.finalMessage();

      const toolUses = final.content.filter((b) => b.type === "tool_use");
      if (toolUses.length === 0) {
        finalText = final.content
          .filter((b) => b.type === "text")
          .map((b) => b.text)
          .join("");
        break;
      }

      messages.push({ role: "assistant", content: final.content });

      const resultBlocks = [];
      for (const tu of toolUses) {
        if (!args.raw) console.log(`\n→ ${tu.name} ${truncate(tu.input ?? {})}`);
        const target = toolToClient.get(tu.name);
        let result;
        if (!target) {
          result = { error: `unknown tool ${tu.name}` };
        } else {
          try {
            result = await target.client.callTool({ name: target.originalName, arguments: tu.input ?? {} });
          } catch (e) {
            result = { error: String(e) };
          }
        }
        if (!args.raw) console.log(`  ← ${truncate(result)}`);
        resultBlocks.push({ type: "tool_result", tool_use_id: tu.id, content: JSON.stringify(result) });
      }
      messages.push({ role: "user", content: resultBlocks });

      if (iter === args.maxIterations - 1) {
        console.error(`hit max iterations (${args.maxIterations}) without a final answer`);
        process.exit(1);
      }
    }
  } finally {
    for (const c of clients) {
      try {
        await c.close();
      } catch {
        // best-effort cleanup, run already succeeded or failed independently
      }
    }
  }

  if (args.raw) process.stdout.write(finalText);
  else console.log("\n✅ " + finalText);
}

main().catch((e) => {
  console.error(String(e?.stack ?? e));
  process.exit(1);
});
