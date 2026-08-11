use crate::block::{Block, Param, Test, TestStep};
use std::path::PathBuf;

pub fn runtime_dir() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".autoqa")
}

pub fn actions_path() -> PathBuf {
    runtime_dir().join("actions.json")
}

fn config_path() -> PathBuf {
    runtime_dir().join("config.json")
}

/// The harness picked on a prior run's first-time prompt, or `None` if
/// never asked yet (missing file, or one that fails to parse).
pub fn read_harness_config() -> Option<crate::harness::Harness> {
    let raw = std::fs::read_to_string(config_path()).ok()?;
    #[derive(serde::Deserialize)]
    struct Config {
        harness: crate::harness::Harness,
    }
    serde_json::from_str::<Config>(&raw).ok().map(|c| c.harness)
}

pub fn write_harness_config(harness: crate::harness::Harness) -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    std::fs::write(
        config_path(),
        serde_json::json!({ "harness": harness }).to_string(),
    )?;
    Ok(())
}

/// Where the review UI's Generate/Run/Pause write and execute the test —
/// anchored under `runtime_dir()` (not the process's cwd), so `autoqa review`
/// works from any directory, not just one that happens to already have a
/// `playwright-tests/` with `@playwright/test` installed.
pub fn playwright_tests_dir() -> PathBuf {
    runtime_dir().join("playwright-tests")
}

/// `session.md` from the most recent `autoqa run` (dirs are named
/// `session-<unix_ms>`, so a plain lexical max gives the latest).
pub fn latest_mcp_session_md() -> Option<PathBuf> {
    let dir = runtime_dir().join("pw-session");
    let latest = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .max_by_key(|p| p.file_name().map(|n| n.to_os_string()))?;
    let md = latest.join("session.md");
    md.is_file().then_some(md)
}

/// The `--query` text from the most recent `autoqa run`, used to title the
/// generated test instead of a generic placeholder.
pub fn latest_query() -> Option<String> {
    std::fs::read_to_string(runtime_dir().join("last-query.txt")).ok()
}

pub fn blocks_dir() -> PathBuf {
    runtime_dir().join("blocks")
}

pub fn params_path() -> PathBuf {
    runtime_dir().join("params.json")
}

/// The lexically-max (== numerically-max, all names are same-width unix-ms
/// timestamps) `session-*` directory name under `pw-session`, if any exist
/// yet — used as a "nothing at or before this belongs to the run about to
/// start" baseline, passed to `node/block-server` so it never mistakes a
/// leftover directory from a past run for the current one (see
/// `agent::block_server_mcp_spec` and `currentSessionBytes` in
/// `node/block-server/server.mjs` for why that matters: Playwright MCP
/// creates its session directory lazily, on its own first tool call, so if
/// `run_block` is called before that — exactly what a TUI-planned block is
/// supposed to do — naively picking "the newest directory on disk" would
/// find only an old, unrelated one).
pub fn max_pw_session_dir_name() -> Option<String> {
    std::fs::read_dir(runtime_dir().join("pw-session"))
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .max()
}

/// Where `node/block-server` appends one JSON line per successful
/// `run_block` call (`{"sessionBytes": <n>, "slug": ..., "bindings": {...}}`
/// — `sessionBytes` is Playwright's `session.md` byte length *at the moment
/// of the call*, read live by the block server, not a timestamp: most
/// tool-call entries in that file carry no usable timestamp of their own,
/// so a byte-offset boundary is the only reliable ordering signal available
/// (see `playwright_codegen::parse_mcp_session_with_offset`). Its own
/// action log, since Playwright MCP's `--save-session` recording can't see
/// a sibling MCP server's actions at all (see
/// `sync_actions_from_latest_mcp_session`'s merge). One fixed "latest run"
/// file, truncated by `clear_run_block_log` before each `autoqa run` starts.
pub fn run_block_log_path() -> PathBuf {
    runtime_dir().join("run-blocks.jsonl")
}

pub fn clear_run_block_log() -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    std::fs::write(run_block_log_path(), "")?;
    Ok(())
}

/// Parses this run's `run_block` log into `(session_bytes, TestStep::Block)`
/// pairs, in call order. Malformed lines (a concurrent write mid-append, or
/// none written at all) are skipped rather than failing the whole sync.
pub fn read_run_block_log() -> Vec<(usize, TestStep)> {
    let Ok(raw) = std::fs::read_to_string(run_block_log_path()) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            let session_bytes = v.get("sessionBytes")?.as_u64()? as usize;
            let slug = v.get("slug")?.as_str()?.to_string();
            let bindings = v
                .get("bindings")
                .and_then(|b| serde_json::from_value(b.clone()).ok())
                .unwrap_or_default();
            Some((session_bytes, TestStep::Block { slug, bindings }))
        })
        .collect()
}

pub fn tests_dir() -> PathBuf {
    runtime_dir().join("tests")
}

pub fn list_tests() -> anyhow::Result<Vec<(String, Test)>> {
    let dir = tests_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut tests = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let test: Test = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        tests.push((slug.to_string(), test));
    }
    Ok(tests)
}

pub fn read_test(slug: &str) -> anyhow::Result<Test> {
    let path = tests_dir().join(format!("{slug}.json"));
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!("test '{slug}' not found at {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn write_test(slug: &str, test: &Test) -> anyhow::Result<()> {
    std::fs::create_dir_all(tests_dir())?;
    std::fs::write(
        tests_dir().join(format!("{slug}.json")),
        serde_json::to_string_pretty(test)?,
    )?;
    Ok(())
}

pub fn delete_test(slug: &str) -> anyhow::Result<()> {
    std::fs::remove_file(tests_dir().join(format!("{slug}.json")))?;
    Ok(())
}

pub fn list_blocks() -> anyhow::Result<Vec<(String, Block)>> {
    let dir = blocks_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut blocks = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let block: Block = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
        blocks.push((slug.to_string(), block));
    }
    Ok(blocks)
}

pub fn read_block(slug: &str) -> anyhow::Result<Block> {
    let path = blocks_dir().join(format!("{slug}.json"));
    let raw = std::fs::read_to_string(&path)
        .map_err(|_| anyhow::anyhow!("block '{slug}' not found at {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

pub fn write_block(slug: &str, block: &Block) -> anyhow::Result<()> {
    std::fs::create_dir_all(blocks_dir())?;
    std::fs::write(
        blocks_dir().join(format!("{slug}.json")),
        serde_json::to_string_pretty(block)?,
    )?;
    Ok(())
}

pub fn delete_block(slug: &str) -> anyhow::Result<()> {
    std::fs::remove_file(blocks_dir().join(format!("{slug}.json")))?;
    Ok(())
}

pub fn read_params() -> Vec<Param> {
    std::fs::read_to_string(params_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_params(params: &[Param]) -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    std::fs::write(params_path(), serde_json::to_string_pretty(params)?)?;
    Ok(())
}

pub fn read_actions() -> Vec<TestStep> {
    std::fs::read_to_string(actions_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn write_actions(entries: &[TestStep]) -> anyhow::Result<()> {
    std::fs::create_dir_all(runtime_dir())?;
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(actions_path(), json)?;
    Ok(())
}

/// Refreshes actions.json from the latest `autoqa run` (MCP) session, unless
/// actions.json was touched more recently — e.g. by hand-editing in the
/// review UI, which should win over re-importing the raw session.
pub fn sync_actions_from_latest_mcp_session() -> anyhow::Result<()> {
    let Some(session_md) = latest_mcp_session_md() else {
        return Ok(());
    };
    let session_modified = std::fs::metadata(&session_md)?.modified()?;
    if let Ok(actions_modified) = std::fs::metadata(actions_path()).and_then(|m| m.modified()) {
        if actions_modified >= session_modified {
            return Ok(());
        }
    }
    import_latest_mcp_session(&session_md)
}

/// Loads the latest `autoqa run` session into the working buffer
/// unconditionally — used by the review UI's "Last run" action to jump to
/// it on demand, discarding whatever's currently in the buffer, regardless
/// of the staleness guard `sync_actions_from_latest_mcp_session` applies on
/// every page load.
pub fn load_latest_mcp_session() -> anyhow::Result<Vec<TestStep>> {
    let session_md = latest_mcp_session_md()
        .ok_or_else(|| anyhow::anyhow!("no `autoqa run` session found yet"))?;
    import_latest_mcp_session(&session_md)?;
    Ok(read_actions())
}

fn import_latest_mcp_session(session_md: &std::path::Path) -> anyhow::Result<()> {
    let md = std::fs::read_to_string(session_md)?;

    // Playwright's own steps, each keyed by the exact byte offset in
    // session.md where its tool-call block ends.
    let playwright_steps = crate::playwright_codegen::parse_mcp_session_with_offset(&md)
        .into_iter()
        .map(|(e, offset)| {
            (
                offset,
                false,
                TestStep::Step {
                    action: e.action,
                    assertion: e.assertion,
                },
            )
        });

    // `run_block` calls autoqa-blocks logged during this run — invisible to
    // Playwright's own recording (see `run_block_log_path`) — each keyed by
    // session.md's byte length *at call time*. Sorting the union by
    // (byte_key, is_block) places a block right after every playwright step
    // that had already been written to session.md by the time it was
    // called, and before every one written afterward; the `is_block`
    // tie-breaker only matters on an exact offset match, where the already-
    // fully-written playwright step should still count as "before".
    let mut steps: Vec<(usize, bool, TestStep)> = read_run_block_log()
        .into_iter()
        .map(|(bytes, step)| (bytes, true, step))
        .collect();
    steps.extend(playwright_steps);
    steps.sort_by_key(|(key, is_block, _)| (*key, *is_block));
    let steps: Vec<TestStep> = steps.into_iter().map(|(_, _, step)| step).collect();

    let steps = crate::block::collapse_known_blocks(steps, &list_blocks().unwrap_or_default());
    write_actions(&steps)

    // Deliberately not cleared here — `clear_run_block_log` already runs
    // once at the start of every `autoqa run` (see agent::cmd_run), and the
    // baseline mechanism (`max_pw_session_dir_name`) already prevents a
    // leftover entry from a *different* run being mistaken for this one.
    // Clearing it here too would make it a one-time-use artifact: the
    // review UI's automatic sync-on-load would consume it, and a later
    // deliberate "load last run" (see `load_latest_mcp_session`) would find
    // nothing left to merge even though the session itself hasn't changed.
}
