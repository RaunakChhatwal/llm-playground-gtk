#![allow(non_snake_case)]
use std::rc::Rc;

use gtk::{glib, prelude::*};
use futures_signals::{signal::{Mutable, SignalExt}, signal_vec::{MutableVec, SignalVecExt, VecDiff}};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest_eventsource::{Event, EventSource};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Notify;

#[derive(Deserialize)]
struct KeyEntry {
    key: String
}

#[derive(Deserialize)]
struct AppConfig {
    keys: Vec<KeyEntry>
}

async fn fetch_response_tokens(exchanges: &[(String, String)], prompt: &str, streaming: Mutable<bool>, mut cb: impl FnMut(&str)) {
    let config = std::fs::read_to_string("/home/raunak/.config/llm-playground/config.json").expect("Config not present.");
    let config: AppConfig = serde_json::from_str(&config)
        .expect("Bad config.");
    let api_key = config.keys[0].key.clone();

    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(&api_key).unwrap());
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

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
        "model": "claude-3-opus-20240229",
        "max_tokens": 2048,
        "stream": true,
        "messages": messages
    });

    let rb = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .body(body.to_string());

    let mut es = EventSource::new(rb).unwrap();
    while let Some(event) = es.next().await {
        if !(*streaming.lock_ref()) {
            break;
        }

        match event {
            Ok(Event::Open) => (), // println!("Connection Open!"),
            Ok(Event::Message(message)) => {
                // {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I"}     }
                let data: serde_json::Value = serde_json::from_str(&message.data).expect("Stream response not in valid JSON format.");
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

fn MessageTextBox(message: String) -> impl IsA<gtk::Widget> {
    let label = gtk::Label::new(Some(&message));
    label.set_css_classes(&["message_label"]);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_selectable(true);

    return label;
}

fn PromptTextBox(clear_prompt: Rc<Notify>) -> gtk::TextView {
    let text_view = gtk::TextView::new();
    text_view.set_height_request(50);
    text_view.set_valign(gtk::Align::Start);
    text_view.buffer().set_text("Enter a prompt here.");
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    glib::spawn_future_local({
        let text_view = text_view.clone();
        async move {
            loop {
                clear_prompt.notified().await;
                text_view.buffer().set_text("Enter a prompt here.");
            }
        }
    });

    return text_view;
}

fn ResponseTextBox(response_tokens: &MutableVec<String>, streaming: Mutable<bool>) -> impl IsA<gtk::Widget> {
    let text_view = gtk::TextView::new();
    text_view.set_editable(false);
    text_view.set_cursor_visible(false);
    text_view.add_css_class("response");
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.set_justification(gtk::Justification::Left);
    text_view.set_valign(gtk::Align::Start);
    text_view.set_left_margin(0);
    text_view.set_right_margin(0);

    glib::spawn_future_local(response_tokens.signal_vec_cloned().for_each({
        let text_view = text_view.clone();
        move |vd| {
            match vd {
                VecDiff::Push { value: token } => text_view.buffer().insert(&mut text_view.buffer().end_iter(), &token),
                VecDiff::Clear {} => text_view.buffer().set_text(""),
                _ => panic!("Not supported.")
            }
            async {}
        }
    }));

    glib::spawn_future_local(streaming.signal().for_each({
        let text_view = text_view.clone();
        move |streaming| {
            text_view.set_visible(streaming);
            async {}
        }
    }));

    return text_view;
}

fn NewButton(exchanges: MutableVec<(String, String)>, clear_prompt: Rc<Notify>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("New")
        .build();

    button.connect_clicked(move |_| {
        let exchanges = exchanges.clone();
        let clear_prompt = clear_prompt.clone();
        glib::spawn_future_local(async move {
            exchanges.lock_mut().clear();
            clear_prompt.notify_one();
        });
    });

    return button;
}

fn SubmitButton(
    exchanges: MutableVec<(String, String)>,
    prompt: impl Fn() -> String + 'static,
    clear_prompt: Rc<Notify>,
    response_tokens: MutableVec<String>,
    streaming: Mutable<bool>
) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Submit")
        .build();

    button.connect_clicked(move |_| {
        let exchanges = exchanges.clone();
        let prompt = prompt();
        let clear_prompt = clear_prompt.clone();
        let response_tokens = response_tokens.clone();
        let streaming = streaming.clone();

        glib::spawn_future_local(async move {
            assert_eq!(response_tokens.lock_ref().len(), 0);
            assert_eq!(*streaming.lock_ref(), false);
            *streaming.lock_mut() = true;
            fetch_response_tokens(
                exchanges.lock_ref().as_ref(),
                &prompt,
                streaming.clone(),
                |token| response_tokens.lock_mut().push_cloned(token.to_string())
            ).await;

            let response = response_tokens.lock_ref().concat();
            if response != "" {    // response may be empty if cancel button is pressed before receiving first token
                *streaming.lock_mut() = false;
                exchanges.lock_mut().push_cloned((prompt, response));
                clear_prompt.notify_one();
                response_tokens.lock_mut().clear();
            }
        });
    });

    return button;
}

fn get_buffer_content(buffer: gtk::TextBuffer) -> String {
    let (start, end) = &buffer.bounds();
    return buffer.text(start, end, false).to_string();
}

fn CancelButton(streaming: Mutable<bool>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Cancel")
        .build();
    
    button.connect_clicked({
        let streaming = streaming.clone();
        move |_| {
            *streaming.lock_mut() = false;
        }
    });

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(streaming);
            async {}
        }
    }));

    return button;
}

pub fn Chat() -> impl IsA<gtk::Widget> {
    let exchanges: MutableVec<(String, String)> = MutableVec::new();

    let scrolled_window = gtk::ScrolledWindow::new();
    let vbox_messages = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let clear_prompt = Rc::new(Notify::new());
    let prompt_text_box = PromptTextBox(clear_prompt.clone());

    let response_tokens = MutableVec::new();
    let streaming = Mutable::new(false);
    let response_text_box = ResponseTextBox(&response_tokens, streaming.clone());

    vbox_messages.append(&prompt_text_box);
    vbox_messages.append(&response_text_box);

    glib::spawn_future_local(exchanges.signal_vec_cloned().for_each({
        let vbox_messages = vbox_messages.clone();
        let prompt_text_box = prompt_text_box.clone();
        move |vd| {
            match vd {
                VecDiff::Push { value: (user_message, assistant_message) } => {
                    MessageTextBox(user_message).insert_before(&vbox_messages, Some(&prompt_text_box));
                    MessageTextBox(assistant_message).insert_before(&vbox_messages, Some(&prompt_text_box));
                },
                VecDiff::RemoveAt { index } => {
                    let mut child = vbox_messages.first_child().unwrap();
                    for _ in 0..index {
                        child = child.next_sibling().unwrap();
                    }
                    vbox_messages.remove(&child);
                },
                VecDiff::Clear {} => {
                    while let Some(child) = vbox_messages.first_child() {
                        if child == prompt_text_box {
                            break;
                        }

                        vbox_messages.remove(&child);
                    }
                }
                _ => panic!("Not supported.")
            }
            async {}
        }
    }));

    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&vbox_messages));
    scrolled_window.set_vexpand(true);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);

    hbox.append(&NewButton(exchanges.clone(), clear_prompt.clone()));

    hbox.append(&SubmitButton(
        exchanges,
        move || get_buffer_content(prompt_text_box.buffer()),
        clear_prompt,
        response_tokens,
        streaming.clone()
    ));

    let dummy_label = gtk::Label::new(None);
    dummy_label.set_hexpand(true);
    hbox.append(&dummy_label);

    let cancel_button = CancelButton(streaming.clone());
    hbox.append(&cancel_button);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    vbox.set_css_classes(&["top-level-box"]);
    vbox.append(&scrolled_window);
    vbox.append(&hbox);
    
    return vbox;
}
