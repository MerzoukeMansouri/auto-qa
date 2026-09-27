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
import { connectMcpServers, parseArgs, truncate } from "../sdk-common.mjs";

const DEFAULT_MODEL = "gemini-3.6-flash";

async function main() {
  const args = parseArgs(process.argv.slice(2), DEFAULT_MODEL);
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

  const { clients, tools: functionDeclarations, toolToClient } = await connectMcpServers(args.mcpConfigFile, {
    Client,
    StdioClientTransport,
    clientNamePrefix: "autoqa-gemini-sdk-",
    buildTool: (name, description, inputSchema) => ({ name, description, parameters: inputSchema }),
  });
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
        model: args.model,
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
