use std::env;

use dotenv::dotenv;

#[derive(Debug, Clone)]
pub struct Settings {
    pub app_host: String,
    pub use_complete_isolation: bool,
}

impl Settings {
    pub fn from_env() -> Self {
        dotenv().ok();

        let app_host = env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1:8000".to_string());
        let use_complete_isolation = env::var("USE_COMPLETE_ISOLATION")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        return Self {
            app_host,
            use_complete_isolation,
        };
    }
}
