use std::rc::Rc;

use futures::StreamExt;
use gtk::{glib, prelude::*};
use futures_signals::{signal::{Mutable, SignalExt}, signal_vec::MutableVec};
use reqwest::{header::{HeaderMap, HeaderValue, CONTENT_TYPE}, RequestBuilder};
use reqwest_eventsource::{Event, EventSource};
use serde_json::json;
use tokio::sync::Notify;

use crate::settings::{Provider, Settings};

fn build_openai_request(key: &str) -> RequestBuilder {
    let mut headers = HeaderMap::new();
    headers.insert("Authorization", HeaderValue::from_str(&format!("Bearer {}", key)).unwrap());
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let request_builder = reqwest::Client::new()
        .post("https://api.openai.com/v1/chat/completions")
        .headers(headers);

    return request_builder;
}

fn build_anthropic_request(key: &str) -> RequestBuilder {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(key).unwrap());
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let request_builder = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers);

    return request_builder;
}

async fn fetch_response_tokens(
    settings: Mutable<Settings>,
    exchanges: &[(String, String)],
    prompt: &str, streaming: Mutable<bool>,
    mut cb: impl FnMut(&str)
) {
    let settings = settings.lock_ref().clone();

    let mut messages: Vec<serde_json::Value> = vec![];
    for (prompt, response) in exchanges {
        messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        messages.push(json!({
            "role": "assistant",
            "content": response
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": prompt
    }));

    let body = json!({
        "model": settings.model,
        "max_tokens": settings.max_tokens,
        "temperature": settings.temperature,
        "stream": true,
        "messages": messages
    });

    let api_key = &settings.api_keys[settings.api_key.expect("No key available.")];
    let request_builder = match api_key.provider {
        Provider::OpenAI => build_openai_request(&api_key.key)
            .body(body.to_string()),
        Provider::Anthropic => build_anthropic_request(&api_key.key)
            .body(body.to_string())
    };

    let mut es = EventSource::new(request_builder).unwrap();
    while let Some(event) = es.next().await {
        if !(*streaming.lock_ref()) {
            break;
        }

        match event {
            Ok(Event::Open) => (),
            Ok(Event::Message(message)) => {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&message.data) {
                    match api_key.provider {
                        Provider::OpenAI => {
                            if let Some(token) = data["choices"][0]["delta"]["content"].as_str() {
                                cb(token);
                            }
                        },
                        Provider::Anthropic => {
                            if let Some(token) = data["delta"]["text"].as_str() {
                                cb(token);
                            }
                        }
                    };
                }
            },
            Err(reqwest_eventsource::Error::StreamEnded) => {
                es.close();
                break;
            },
            Err(err) => {
                eprintln!("Error: {}", err);
                es.close();
                break;
            }
        }
    }
}

pub fn SubmitButton(
    exchanges: MutableVec<(String, String)>,
    prompt: impl Fn() -> String + 'static,
    settings: Mutable<Settings>,
    clear_prompt: Rc<Notify>,
    response_tokens: MutableVec<String>,
    streaming: Mutable<bool>
) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Submit")
        .build();

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(!streaming);
            async {}
        }
    }));

    button.connect_clicked(move |_| {
        let settings = settings.clone();
        let exchanges = exchanges.clone();
        let prompt = prompt();
        let clear_prompt = clear_prompt.clone();
        let response_tokens = response_tokens.clone();
        let streaming = streaming.clone();

        glib::spawn_future_local(async move {
            assert!(response_tokens.lock_ref().is_empty());
            assert_eq!(*streaming.lock_ref(), false);
            *streaming.lock_mut() = true;
            fetch_response_tokens(
                settings,
                exchanges.lock_ref().as_ref(),
                &prompt,
                streaming.clone(),
                |token| response_tokens.lock_mut().push_cloned(token.to_string())
            ).await;

            if !response_tokens.lock_ref().is_empty() {    // response may be empty if cancel button is pressed before receiving first token
                *streaming.lock_mut() = false;
                exchanges.lock_mut().push_cloned((prompt, response_tokens.lock_ref().concat()));
                clear_prompt.notify_one();
                response_tokens.lock_mut().clear();
            }
        });
    });

    return button;
}