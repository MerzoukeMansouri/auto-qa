#!/usr/bin/env node
// Own agent loop against the Gemini API directly (no gemini CLI subprocess):
// connects to whatever MCP servers are listed in --mcp-config-file, turns
// their tools into genai function-declarations, and drives the
// call-model -> run-tool -> feed-result-back loop by hand. stdout is the
// harness's own log format — no undocumented CLI stream schema to guess at.
import { GoogleGenAI } from "@google/genai";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import fs from "node:fs";

function parseArgs(argv) {
  const args = { query: null, systemPromptFile: null, mcpConfigFile: null, maxIterations: 50, raw: false };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--system-prompt-file") args.systemPromptFile = argv[++i];
    else if (a === "--mcp-config-file") args.mcpConfigFile = argv[++i];
    else if (a === "--max-iterations") args.maxIterations = parseInt(argv[++i], 10);
    else if (a === "--raw") args.raw = true;
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
    return { clients: [], functionDeclarations: [], toolToClient: new Map() };
  }
  const config = JSON.parse(fs.readFileSync(mcpConfigFile, "utf8"));
  const servers = config.mcpServers ?? {};
  const clients = [];
  const functionDeclarations = [];
  const toolToClient = new Map();
  for (const [name, spec] of Object.entries(servers)) {
    const transport = new StdioClientTransport({ command: spec.command, args: spec.args ?? [] });
    const client = new Client({ name: `autoqa-gemini-sdk-${name}`, version: "1.0.0" });
    await client.connect(transport);
    const { tools } = await client.listTools();
    for (const t of tools) {
      const qualifiedName = `mcp__${name}__${t.name}`;
      functionDeclarations.push({
        name: qualifiedName,
        description: t.description ?? "",
        parameters: t.inputSchema ?? { type: "object", properties: {} },
      });
      toolToClient.set(qualifiedName, { client, originalName: t.name });
    }
    clients.push(client);
  }
  return { clients, functionDeclarations, toolToClient };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args.query) {
    console.error("missing query argument");
    process.exit(1);
  }
  const apiKey = process.env.GEMINI_API_KEY;
  if (!apiKey) {
    console.error("GEMINI_API_KEY not set");
    process.exit(1);
  }
  const systemInstruction = args.systemPromptFile
    ? fs.readFileSync(args.systemPromptFile, "utf8")
    : undefined;

  const { clients, functionDeclarations, toolToClient } = await connectMcpServers(args.mcpConfigFile);
  const ai = new GoogleGenAI({ apiKey });

  const contents = [{ role: "user", parts: [{ text: args.query }] }];
  const genConfig = {
    systemInstruction,
    ...(functionDeclarations.length ? { tools: [{ functionDeclarations }] } : {}),
  };

  let finalText = "";
  try {
    for (let iter = 0; iter < args.maxIterations; iter++) {
      const stream = await ai.models.generateContentStream({
        model: "gemini-3.6-flash",
        contents,
        config: genConfig,
      });

      let text = "";
      // Keep each streamed part object as-is (not just its .functionCall) —
      // Gemini 3 attaches a `thoughtSignature` alongside `functionCall` on
      // the same part and rejects a follow-up turn that dropped it
      // (400 "Function call is missing a thought_signature"), so the part
      // must be echoed back verbatim, not reconstructed from just the name/args.
      const modelParts = [];
      const functionCalls = [];
      for await (const chunk of stream) {
        const parts = chunk.candidates?.[0]?.content?.parts ?? [];
        for (const part of parts) {
          if (part.text) {
            text += part.text;
            if (!args.raw) process.stdout.write("💬 " + part.text + "\n");
          }
          if (part.functionCall) {
            functionCalls.push(part.functionCall);
            modelParts.push(part);
          }
        }
      }

      if (functionCalls.length === 0) {
        finalText = text;
        break;
      }

      contents.push({ role: "model", parts: modelParts });

      const responseParts = [];
      for (const fc of functionCalls) {
        if (!args.raw) console.log(`→ ${fc.name} ${truncate(fc.args ?? {})}`);
        const target = toolToClient.get(fc.name);
        let result;
        if (!target) {
          result = { error: `unknown tool ${fc.name}` };
        } else {
          try {
            result = await target.client.callTool({ name: target.originalName, arguments: fc.args ?? {} });
          } catch (e) {
            result = { error: String(e) };
          }
        }
        if (!args.raw) console.log(`  ← ${truncate(result)}`);
        responseParts.push({ functionResponse: { name: fc.name, response: { result } } });
      }
      contents.push({ role: "user", parts: responseParts });

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
