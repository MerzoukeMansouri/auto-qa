#[derive(serde::Serialize)]
pub struct ActionResult {
    pub url: String,
    pub screenshot_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<(u32, u32)>,
}

pub fn print(result: &ActionResult) {
    println!("{}", serde_json::to_string(result).unwrap());
}
