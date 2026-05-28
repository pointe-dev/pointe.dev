use crate::langfuse::LangfuseClient;

pub struct AppState {
    pub anthropic_key: String,
    pub http: reqwest::Client,
    pub system_prompt: String,
    pub langfuse: Option<LangfuseClient>,
}
