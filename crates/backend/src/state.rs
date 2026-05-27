pub struct AppState {
    pub anthropic_key: String,
    pub openrouter_key: String,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new() -> Self {
        let anthropic_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        let openrouter_key = std::env::var("OPENROUTER_API_KEY")
            .expect("OPENROUTER_API_KEY must be set");
        let http = reqwest::Client::new();
        Self { anthropic_key, openrouter_key, http }
    }
}
