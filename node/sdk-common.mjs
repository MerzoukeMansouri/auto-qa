// Shared bits for the SDK harnesses (claude-sdk, gemini-sdk): CLI arg
// parsing, log-line truncation, and connecting to every MCP server listed in
// --mcp-config-file, turning their tools into qualified `mcp__<server>__<tool>`
// definitions (matching the mcp__ prefix convention the shared system prompt
// already writes tool references in — see SYSTEM_PROMPT in src/agent.rs).
//
// No `@modelcontextprotocol/sdk` import here on purpose: this file is
// deployed standalone (see write_sdk_files in src/harness.rs, which embeds
// it via include_str! next to each harness's own index.mjs), so it must
// resolve with nothing but Node builtins — `Client`/`StdioClientTransport`
// are passed in by the caller, which already depends on that package.
import fs from "node:fs";

export function parseArgs(argv, defaultModel) {
  const args = {
    query: null,
    systemPromptFile: null,
    mcpConfigFile: null,
    maxIterations: 50,
    raw: false,
    model: defaultModel,
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

export function truncate(v, n = 300) {
  const s = typeof v === "string" ? v : JSON.stringify(v);
  return s.length > n ? s.slice(0, n) + "…" : s;
}

// `Client`/`StdioClientTransport`: the caller's own `@modelcontextprotocol/sdk`
// imports (see note above on why they're injected, not imported here).
// `clientNamePrefix`: e.g. "autoqa-claude-sdk-" / "autoqa-gemini-sdk-".
// `buildTool`: (qualifiedName, description, inputSchema) => tool-def object,
// since Claude and Gemini each want a differently-shaped tool descriptor.
export async function connectMcpServers(mcpConfigFile, { Client, StdioClientTransport, clientNamePrefix, buildTool }) {
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
    const client = new Client({ name: `${clientNamePrefix}${name}`, version: "1.0.0" });
    await client.connect(transport);
    const { tools: serverTools } = await client.listTools();
    for (const t of serverTools) {
      const qualifiedName = `mcp__${name}__${t.name}`;
      tools.push(buildTool(qualifiedName, t.description ?? "", t.inputSchema ?? { type: "object", properties: {} }));
      toolToClient.set(qualifiedName, { client, originalName: t.name });
    }
    clients.push(client);
  }
  return { clients, tools, toolToClient };
}
