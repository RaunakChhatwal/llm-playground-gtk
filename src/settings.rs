use std::env;

use futures_signals::signal::Mutable;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub enum Provider {
    OpenAI,
    Anthropic
}

impl ToString for Provider {
    fn to_string(&self) -> String {
        match *self {
            Provider::OpenAI => "OpenAI".to_string(),
            Provider::Anthropic => "Anthropic".to_string()
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct APIKey {
    pub name: String,
    pub key: String,
    pub provider: Provider
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Settings {
    pub temperature: f64,
    pub max_tokens: u32,
    pub model: String,
    pub api_key: Option<usize>,
    pub api_keys: Vec<APIKey>
}

pub fn load_settings() -> Mutable<Settings> {
    let config_path = env::var("HOME").unwrap() + "/.config/llm-playground/config.json";
    let settings: Settings;
    if let Ok(settings_str) = std::fs::read_to_string(config_path) {
        settings = serde_json::from_str(&settings_str).expect("Bad config");
    } else {
        settings = Settings {
            temperature: 1.0,
            max_tokens: 1024,
            model: "".to_string(),
            api_key: None,
            api_keys: vec![]
        };
        save_settings(&settings);
    }

    return Mutable::new(settings);
}

pub fn save_settings(settings: &Settings) {
    let config_path = env::var("HOME").unwrap() + "/.config/llm-playground/config.json";
    std::fs::write(config_path, &serde_json::to_string(&settings).unwrap()).unwrap();
}