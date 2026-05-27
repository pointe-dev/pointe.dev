pub struct AppState {
    pub anthropic_key: String,
    pub http: reqwest::Client,
}

impl AppState {
    pub fn new() -> Self {
        let anthropic_key = std::env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY must be set");
        let http = reqwest::Client::new();
        Self { anthropic_key, http }
    }
}
