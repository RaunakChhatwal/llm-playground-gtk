#![allow(non_snake_case)]
use std::{cell::RefCell, rc::Rc};

use gtk::{gdk, glib, prelude::*};
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

async fn fetch_response_tokens(prompts: &Vec<String>, cancel: Rc<RefCell<bool>>, mut cb: impl FnMut(&str) -> ()) {
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
        "messages": prompts
            .iter()
            .enumerate()
            .map(|(i, prompt)|
                json!({
                    "role": if i % 2 == 0 { "user" } else { "assistant" },
                    "content": prompt }))
            .collect::<Vec<serde_json::Value>>()
    });

    let rb = reqwest::Client::new()
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .body(body.to_string());

    let mut es = EventSource::new(rb).unwrap();
    while let Some(event) = es.next().await {
        if *cancel.borrow() {
            break;
        }

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
    std::fs::write("/home/raunak/misc/rust/gtk/llm-playground/variables.json", json!({
        "terminal": std::env::vars()
            .map(|(name, value)| json!({
                "name": name,
                "value": value
            }))
            .collect::<Vec<serde_json::Value>>()
    }).to_string()).unwrap();

    let application = gtk::Application::builder()
        .application_id("us.raunak.llm-playground")
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

fn MessageTextBox(response: gtk::TextBuffer) -> gtk::TextView {
    let text_view = gtk::TextView::new();
    text_view.set_valign(gtk::Align::Start);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.set_buffer(Some(&response));

    return text_view;
}

fn PromptTextBox() -> (Mutable<gtk::TextBuffer>, impl IsA<gtk::Widget>) {
    let text_view = gtk::TextView::new();
    text_view.set_height_request(50);
    text_view.set_valign(gtk::Align::Start);
    text_view.buffer().set_text("Enter a prompt here.");
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    return (Mutable::new(text_view.buffer()), text_view);
}

fn ResponseTextBox(response: &Mutable<String>) -> impl IsA<gtk::Widget> {
    let label = gtk::Label::new(None);
    label.add_css_class("response_label");
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_justify(gtk::Justification::Left);
    label.set_valign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_selectable(true);


    glib::spawn_future_local(response.signal_cloned().for_each({
        let label = label.clone();
        move |response| {
            label.set_text(&response);
            async {}
        }
    }));

    return label;
}

fn NewButton(notify: Rc<Notify>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("New")
        .build();

    button.connect_clicked(move |_| notify.notify_one() );

    return button;
}

fn SubmitButton(
    message_buffers: MutableVec<gtk::TextBuffer>,
    prompt_buffer: Mutable<gtk::TextBuffer>,
    response: Mutable<String>,
    cancel: Rc<RefCell<bool>>
) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Submit")
        .build();

    button.connect_clicked(move |_| {
        let response = response.clone();
        let prompt_buffer = prompt_buffer.clone();
        let message_buffers = message_buffers.clone();
        let cancel = cancel.clone();
        let messages: Vec<String> = message_buffers
            .lock_ref()
            .iter()
            .chain(std::iter::once(prompt_buffer.lock_ref().as_ref()))
            .map(|prompt_buffer| {
                let (start, end) = &prompt_buffer.bounds();
                return prompt_buffer.text(start, end, false).to_string();
            })
            .collect();

        glib::spawn_future_local(async move {
            assert_eq!(*response.lock_ref(), "");
            assert_eq!(*cancel.borrow_mut(), false);
            fetch_response_tokens(&messages, cancel.clone(), |token| { *response.lock_mut() += token; }).await;

            let response_text = response.lock_ref().clone();
            *response.lock_mut() = String::new();
            *cancel.borrow_mut() = false;

            let user_message_buffer = gtk::TextBuffer::new(None);
            user_message_buffer.set_text(messages.last().unwrap());
            message_buffers.lock_mut().push_cloned(user_message_buffer);
            prompt_buffer.lock_mut().set_text("Enter a prompt here.");

            let response_buffer = gtk::TextBuffer::new(None);
            response_buffer.set_text(&response_text);
            message_buffers.lock_mut().push_cloned(response_buffer);
        });
    });

    return button;
}

fn CancelButton(notify: Rc<RefCell<bool>>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Cancel")
        .build();
    
    button.connect_clicked(move |_| {
        *notify.borrow_mut() = true;
    });

    return button;
}

fn App(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_default_size(450, 550);

    window.set_title(Some("Chat Playground"));

    let scrolled_window = gtk::ScrolledWindow::new();
    let vbox_messages = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let message_buffers: MutableVec<gtk::TextBuffer> = MutableVec::new();
    let (prompt_buffer, prompt_text_box) = PromptTextBox();
    // let message_buffers = Rc::new(RefCell::new(vec![message_buffer]));

    let response = Mutable::new(String::new());
    let response_text_box = ResponseTextBox(&response);

    glib::spawn_future_local(message_buffers.signal_vec_cloned().for_each({
        let vbox_messages = vbox_messages.clone();
        let prompt_text_box = prompt_text_box.clone();
        move |vd| {
            if let VecDiff::Push { value: message_buffer } = vd {
                let message_text_box = MessageTextBox(message_buffer.clone());
                message_text_box.insert_before(&vbox_messages, Some(&prompt_text_box));
                message_buffer.insert_interactive(&mut message_buffer.end_iter(), "\n", true);  // hack to prevent a layout bug
            } else if let VecDiff::Clear {} = vd {
                while let Some(message_text_box) = vbox_messages.first_child() {
                    if message_text_box == prompt_text_box {
                        break;
                    }

                    vbox_messages.remove(&message_text_box);
                }
            } else {
                panic!("Not implemented.");
            }

            async {}
        }
    }));


    vbox_messages.append(&prompt_text_box);
    vbox_messages.append(&response_text_box);

    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&vbox_messages));
    scrolled_window.set_vexpand(true);
    // scrolled_window.set_valign(gtk::Align::Fill);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);

    let notify = Rc::new(Notify::new());
    hbox.append(&NewButton(notify.clone()));
    glib::spawn_future_local({
        let message_buffers = message_buffers.clone();
        async move {
            loop {
                notify.notified().await;
                message_buffers.lock_mut().clear();
            }
        }
    });

    let cancel = Rc::new(RefCell::new(false));
    hbox.append(&SubmitButton(message_buffers, prompt_buffer, response.clone(), cancel.clone()));

    let dummy_label = gtk::Label::new(None);
    dummy_label.set_hexpand(true);
    hbox.append(&dummy_label);

    let cancel_button = CancelButton(cancel);
    hbox.append(&cancel_button);

    glib::spawn_future_local(response.signal_cloned().eq(String::new()).for_each({
        let response_text_box = response_text_box.clone();
        move |is_empty| {
            response_text_box.clone().set_visible(!is_empty);
            cancel_button.clone().set_visible(!is_empty);
            async {}
        }
    }));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    vbox.set_css_classes(&["top-level-box"]);
    vbox.append(&scrolled_window);
    // vbox.append(&dummy_label);
    vbox.append(&hbox);

    window.set_child(Some(&vbox));

    application.connect_activate(move |_| {
        window.present();
    });
}