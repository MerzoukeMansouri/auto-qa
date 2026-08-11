use crate::action_entry::ActionEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A named, reusable sequence of steps — authored once in the review UI,
/// then referenced (live, not copied) from any test's step list, and
/// replayable directly by the agent at run time via the `run_block` MCP
/// tool. Steps may contain `{{placeholder}}` tokens, filled in per-use via
/// a `TestStep::Block::bindings` map.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Block {
    pub name: String,
    pub steps: Vec<ActionEntry>,
}

/// A named value in the shared param library (`params.json`) — bound to a
/// block's `{{placeholder}}` tokens by name, so editing a param's value
/// here updates every block/test binding that references it.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Param {
    pub name: String,
    pub value: String,
}

/// One entry in a test's step list (`actions.json`). Either a literal step
/// or a live reference to a `Block` by slug, freely interleaved with plain
/// steps and with each other. `bindings` maps a placeholder name (as it
/// appears inside the block's `{{...}}` tokens) to a `Param` name.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TestStep {
    Step {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        action: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        assertion: String,
    },
    Block {
        slug: String,
        #[serde(default)]
        bindings: HashMap<String, String>,
    },
}

/// Rewrites any contiguous run of literal `TestStep::Step`s that exactly
/// matches a known block's steps into a `TestStep::Block` reference —
/// applied after importing a raw `autoqa run` MCP session, so a run that
/// happened to replay a saved block's exact steps (whether the agent called
/// `run_block` or just improvised the same actions) shows up in the review
/// UI as the block, not as its expansion. Only literal blocks (no
/// `{{placeholder}}` tokens) are matched — a recorded action carries a
/// concrete value, so there's no way to recover which param a placeholder
/// would have been bound to. Blocks are tried longest-steps-first at each
/// position so a bigger block wins over a shorter one that happens to be a
/// prefix of it.
pub fn collapse_known_blocks(steps: Vec<TestStep>, blocks: &[(String, Block)]) -> Vec<TestStep> {
    let mut candidates: Vec<&(String, Block)> = blocks
        .iter()
        .filter(|(_, b)| {
            !b.steps.is_empty()
                && b.steps
                    .iter()
                    .all(|s| !s.action.contains("{{") && !s.assertion.contains("{{"))
        })
        .collect();
    candidates.sort_by_key(|(_, b)| std::cmp::Reverse(b.steps.len()));

    let mut result = Vec::new();
    let mut i = 0;
    'outer: while i < steps.len() {
        for (slug, block) in &candidates {
            let n = block.steps.len();
            if i + n > steps.len() {
                continue;
            }
            let matches = (0..n).all(|k| match &steps[i + k] {
                TestStep::Step { action, assertion } => {
                    *action == block.steps[k].action && *assertion == block.steps[k].assertion
                }
                TestStep::Block { .. } => false,
            });
            if matches {
                result.push(TestStep::Block {
                    slug: slug.clone(),
                    bindings: HashMap::new(),
                });
                i += n;
                continue 'outer;
            }
        }
        result.push(steps[i].clone());
        i += 1;
    }
    result
}

/// A named, saved test — `~/.autoqa/tests/<slug>.json`. The review UI's
/// "current" test (`actions.json`, edited/validated/run in place) is a
/// working buffer: opening a saved test loads its steps into that buffer,
/// saving writes the buffer's current steps back out under a slug.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Test {
    pub name: String,
    pub steps: Vec<TestStep>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(action: &str, assertion: &str) -> TestStep {
        TestStep::Step {
            action: action.to_string(),
            assertion: assertion.to_string(),
        }
    }

    #[test]
    fn collapses_exact_match_into_block_ref() {
        let block = Block {
            name: "Buy Milk".to_string(),
            steps: vec![
                ActionEntry {
                    action: "goto".to_string(),
                    assertion: String::new(),
                },
                ActionEntry {
                    action: "fill milk".to_string(),
                    assertion: "assert milk".to_string(),
                },
            ],
        };
        let steps = vec![
            step("goto", ""),
            step("fill milk", "assert milk"),
            step("unrelated", ""),
        ];
        let collapsed = collapse_known_blocks(steps, &[("buy-milk".to_string(), block)]);
        assert_eq!(collapsed.len(), 2);
        assert!(matches!(&collapsed[0], TestStep::Block { slug, .. } if slug == "buy-milk"));
        assert!(matches!(&collapsed[1], TestStep::Step { action, .. } if action == "unrelated"));
    }

    #[test]
    fn leaves_non_matching_steps_untouched() {
        let block = Block {
            name: "Buy Milk".to_string(),
            steps: vec![ActionEntry {
                action: "goto".to_string(),
                assertion: String::new(),
            }],
        };
        let steps = vec![step("something else", "")];
        let collapsed = collapse_known_blocks(steps.clone(), &[("buy-milk".to_string(), block)]);
        assert_eq!(collapsed.len(), steps.len());
        assert!(matches!(&collapsed[0], TestStep::Step { .. }));
    }

    #[test]
    fn skips_blocks_with_placeholders() {
        let block = Block {
            name: "Login".to_string(),
            steps: vec![ActionEntry {
                action: "fill('{{username}}')".to_string(),
                assertion: String::new(),
            }],
        };
        let steps = vec![step("fill('{{username}}')", "")];
        let collapsed = collapse_known_blocks(steps, &[("login".to_string(), block)]);
        assert!(matches!(&collapsed[0], TestStep::Step { .. }));
    }
}
