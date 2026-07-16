use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ActionEntry {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assert_kind: Option<String>,
}

impl ActionEntry {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            ..Default::default()
        }
    }
}
