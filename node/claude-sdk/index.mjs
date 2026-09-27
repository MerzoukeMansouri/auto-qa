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
import { connectMcpServers, parseArgs, truncate } from "../sdk-common.mjs";

const DEFAULT_MODEL = "claude-haiku-4-5";
const MAX_TOKENS = 8192;

async function main() {
  const args = parseArgs(process.argv.slice(2), DEFAULT_MODEL);
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

  const { clients, tools, toolToClient } = await connectMcpServers(args.mcpConfigFile, {
    Client,
    StdioClientTransport,
    clientNamePrefix: "autoqa-claude-sdk-",
    buildTool: (name, description, inputSchema) => ({ name, description, input_schema: inputSchema }),
  });
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
