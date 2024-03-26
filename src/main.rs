#![allow(non_snake_case)]
use gtk::{gdk, glib, prelude::*};
use futures_signals::signal::{Mutable, SignalExt};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest_eventsource::{Event, EventSource};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct KeyEntry {
    key: String
}

#[derive(Deserialize)]
struct AppConfig {
    keys: Vec<KeyEntry>
}

async fn fetch_response_tokens(prompt: &str, cb: impl Fn(&str) -> ()) {
    let config: AppConfig = serde_json::from_str(&std::fs::read_to_string("/home/raunak/.config/llm-playground/config.json").expect("No keys present.")).expect("Bad config.");
    let api_key = config.keys[0].key.clone();
    // println!("{}", api_key);
    // return Ok(());

    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(&api_key).unwrap());
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let body = json!({
        "model": "claude-3-opus-20240229",
        "max_tokens": 2048,
        "stream": true,
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    let rb = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .body(body.to_string());

    let mut es = EventSource::new(rb).unwrap();
    while let Some(event) = es.next().await {
        match event {
            Ok(Event::Open) => (), // println!("Connection Open!"),
            Ok(Event::Message(message)) => {
                // {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I"}     }
                let data: serde_json::Value = serde_json::from_str(&message.data).unwrap();
                if let Some(token) = data["delta"]["text"].as_str() {
                    cb(token);
                }
            },
            Err(reqwest_eventsource::Error::StreamEnded) => es.close(),
            Err(err) => {
                eprintln!("Error: {}", err);
                es.close();
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let application = gtk::Application::builder()
        .application_id("us.raunak.chatplayground")
        .build();

    application.connect_startup(|app| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION
        );

        App(app);
    });

    application.run();
    return Ok(());
}

fn PromptTextBox() -> (gtk::TextBuffer, impl IsA<gtk::Widget>) {
    let text_view = gtk::TextView::new();
    text_view.buffer().set_text("Enter a prompt here.");
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    let scrolled_window = gtk::ScrolledWindow::new();
    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&text_view));
    scrolled_window.set_height_request(75);

    return (text_view.buffer(), scrolled_window);
}

fn ResponseTextBox(response: &Mutable<String>) -> impl IsA<gtk::Widget> {
    let label = gtk::Label::new(None);
    label.add_css_class("response_label");
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_justify(gtk::Justification::Left);
    label.set_valign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_vexpand(true);
    label.set_selectable(true);

    let scrolled_window = gtk::ScrolledWindow::new();
    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&label));
    scrolled_window.set_height_request(75);

    glib::spawn_future_local(response.signal_cloned().for_each(move |response| {
        label.set_text(&response);
        async {}
    }));

    scrolled_window
}

fn HistoryButton() -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("History")
        .build();

    button.connect_clicked(|_| println!("Hello world!") );

    return button;
}

fn SubmitButton(prompt_buffer: gtk::TextBuffer, response: Mutable<String>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Submit")
        .build();

    button.connect_clicked(move |_| {
        let response = response.clone();
        let (start, end) = &prompt_buffer.bounds();
        let prompt = prompt_buffer.text(start, end, false).to_string();
        glib::spawn_future_local(async move {
            fetch_response_tokens(&prompt, |token| { *response.lock_mut() += token; }).await;
        });
    });

    return button;
}

fn App(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_default_size(650, 500);

    window.set_title(Some("Chat Playground"));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let response = Mutable::new(String::new());
    let (prompt_buffer, prompt_text_box) = PromptTextBox();
    let response_text_box = ResponseTextBox(&response);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    // hbox.set_valign(gtk::Align::End);
    hbox.append(&HistoryButton());
    hbox.append(&SubmitButton(prompt_buffer, response));

    vbox.append(&prompt_text_box);
    vbox.append(&response_text_box);
    vbox.append(&hbox);
    window.set_child(Some(&vbox));

    application.connect_activate(move |_| {
        window.present();
    });
}