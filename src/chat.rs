#![allow(non_snake_case)]
use std::{cell::RefCell, rc::Rc};

use gtk::{glib::{self, clone}, prelude::*};
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
                break;
            }
        }
    }
}

fn MessageTextBox(message: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(message));
    label.set_css_classes(&["message_label"]);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_selectable(true);

    return label;
}

fn EditableMessageTextBox(message: &str) -> gtk::TextView {
    let text_view = gtk::TextView::new();
    // text_view.set_height_request(50);
    text_view.set_valign(gtk::Align::Start);
    text_view.buffer().set_text(message);
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);

    return text_view;
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

fn NewButton(exchanges: MutableVec<(String, String)>, streaming: Mutable<bool>, clear_prompt: Rc<Notify>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("New")
        .build();

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(!streaming);
            async {}
        }
    }));    

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

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(!streaming);
            async {}
        }
    }));

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

fn get_buffer_content(buffer: &gtk::TextBuffer) -> String {
    let (start, end) = &buffer.bounds();
    return buffer.text(start, end, false).to_string();
}

fn CancelButton(streaming: Mutable<bool>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Cancel")
        .build();
    
    button.connect_clicked(clone!(
        @strong streaming
        => move |_| {
            *streaming.lock_mut() = false;
        }
    ));

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(streaming);
            async {}
        }
    }));

    return button;
}

fn EditModeButton(edit_mode: Mutable<bool>, streaming: Mutable<bool>) -> impl IsA<gtk::Widget> {
    let button = gtk::Button::builder()
        .label("Edit Mode")
        .build();
    
    button.connect_clicked(clone!(
        @weak button,
        @strong edit_mode
        => move |_| {
            let val = *edit_mode.lock_ref();
            *edit_mode.lock_mut() = !val;
            if !val {
                button.set_label("Static Mode");
            } else {
                button.set_label("Edit Mode");
            }
        }
    ));

    glib::spawn_future_local(streaming.signal().for_each({
        let button = button.clone();
        move |streaming| {
            button.set_visible(!streaming);
            async {}
        }
    }));

    return button;
}

fn HeaderOption(label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_label(label);
    button.set_css_classes(&["flat", "close_button"]);
    button.remove_css_class("button");

    return button;
}

type ExchangeWidget = gtk::Box;
fn Exchange(
    user_message: String,
    assistant_message: String,
    edit_mode: Mutable<bool>,
    edit_exchange: impl Fn((String, String)) + 'static,
    delete_exchange: impl Fn() + 'static
) -> ExchangeWidget {
    let exchange = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let exchange_header = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    exchange_header.set_css_classes(&["exchange_header"]);
    let dummy_label = gtk::Label::new(None);
    dummy_label.set_hexpand(true);
    exchange_header.append(&dummy_label);

    let edit_button = HeaderOption("Edit");
    exchange_header.append(&edit_button);

    let delete_button = HeaderOption("Delete");
    delete_button.connect_clicked(move |_| delete_exchange());
    exchange_header.append(&delete_button);

    let done_button = HeaderOption("Done");
    done_button.set_visible(false);
    exchange_header.append(&done_button);

    exchange.append(&exchange_header);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let user_text_box = MessageTextBox(&user_message);
    let editable_user_text_box = EditableMessageTextBox(&user_message);
    vbox.append(&user_text_box);
    let assistant_text_box = MessageTextBox(&assistant_message);
    let editable_assistant_text_box = EditableMessageTextBox(&assistant_message);
    vbox.append(&assistant_text_box);
    vbox.set_hexpand(true);
    exchange.append(&vbox);

    glib::spawn_future_local(edit_mode.signal().for_each({
        let vbox = vbox.clone();
        move |edit_mode| {
            exchange_header.set_visible(edit_mode);
            if edit_mode {
                vbox.set_spacing(5);
            } else {
                vbox.set_spacing(10);
            }
            async {}
        }
    }));

    edit_button.connect_clicked(clone!(
        @weak edit_button,
        @weak delete_button,
        @weak done_button,
        @weak vbox,
        @weak user_text_box,
        @weak assistant_text_box,
        @weak editable_user_text_box,
        @weak editable_assistant_text_box
        => move |_| {
            edit_button.set_visible(false);
            delete_button.set_visible(false);
            vbox.remove(&user_text_box);
            vbox.remove(&assistant_text_box);
            editable_user_text_box.buffer().set_text(&user_text_box.label().to_string());
            editable_assistant_text_box.buffer().set_text(&assistant_text_box.label().to_string());
            vbox.append(&editable_user_text_box);
            vbox.append(&editable_assistant_text_box);
            done_button.set_visible(true);
        }
    ));

    done_button.connect_clicked(clone!(
        @weak done_button
        => move |_| {
            done_button.set_visible(false);
            vbox.remove(&editable_user_text_box);
            vbox.remove(&editable_assistant_text_box);
            user_text_box.set_label(&get_buffer_content(&editable_user_text_box.buffer()));
            assistant_text_box.set_label(&get_buffer_content(&editable_assistant_text_box.buffer()));
            edit_exchange((user_text_box.label().to_string(), assistant_text_box.label().to_string()));
            vbox.append(&editable_user_text_box);
            vbox.append(&editable_assistant_text_box);
            edit_button.set_visible(true);
            delete_button.set_visible(true);
        }
    ));

    return exchange;
}

fn edit_exchange(exchanges: &MutableVec<(String, String)>, new_exchange: (String, String), deletions: &Rc<RefCell<Vec<usize>>>, id: usize) {
    let mut index = id;
    for deletion in (*(*deletions)).borrow().iter() {
        if index >= *deletion {
            index -= 1;
        }
    }

    exchanges.lock_mut().set_cloned(index, new_exchange);
}

fn delete_exchange(exchanges: &MutableVec<(String, String)>, deletions: &Rc<RefCell<Vec<usize>>>, id: usize) {
    let mut index = id;
    for deletion in (*(*deletions)).borrow().iter() {
        if index >= *deletion {
            index -= 1;
        }
    }

    exchanges.lock_mut().remove(index);
}

fn Exchanges(
    exchanges: MutableVec<(String, String)>,
    response_tokens: MutableVec<String>,
    streaming: Mutable<bool>,
    edit_mode: Mutable<bool>,
    clear_prompt: Rc<Notify>
) -> (gtk::TextBuffer, gtk::Box) {
    let id_counter = Rc::new(RefCell::new(0usize));
    let deletions: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
    let exchanges_memo: Rc<RefCell<Vec<(usize, ExchangeWidget)>>> = Rc::new(RefCell::new(vec![]));

    let vbox_exchanges = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let prompt_text_box = PromptTextBox(clear_prompt.clone());
    let response_text_box = ResponseTextBox(&response_tokens, streaming.clone());

    vbox_exchanges.append(&prompt_text_box);
    vbox_exchanges.append(&response_text_box);

    glib::spawn_future_local(exchanges.signal_vec_cloned().for_each({
        let vbox_exchanges = vbox_exchanges.clone();
        let prompt_text_box = prompt_text_box.clone();
        let exchanges = exchanges.clone();
        let edit_mode = edit_mode.clone();
        move |vd| {
            match vd {
                VecDiff::UpdateAt { index: _, value: _ } => {},
                VecDiff::Push { value: (user_message, assistant_message) } => {
                    let id = *(*id_counter).borrow();
                    *id_counter.borrow_mut() += 1;
                    let edit_mode = edit_mode.clone();
                    let exchange = Exchange(user_message, assistant_message, edit_mode, {
                        let exchanges = exchanges.clone();
                        let deletions = deletions.clone();
                        move |new_exchange| edit_exchange(&exchanges, new_exchange, &deletions, id)
                    }, {
                        let exchanges = exchanges.clone();
                        let deletions = deletions.clone();
                        move || delete_exchange(&exchanges, &deletions, id)
                    });
                    exchange.insert_before(&vbox_exchanges, Some(&prompt_text_box));
                    exchanges_memo.borrow_mut().push((id, exchange));
                },
                VecDiff::RemoveAt { index } => {
                    let (id, child) = exchanges_memo.borrow_mut().remove(index);
                    deletions.borrow_mut().push(id);
                    vbox_exchanges.remove(&child);
                },
                VecDiff::Pop {} => {
                    let (id, child) = exchanges_memo.borrow_mut().pop().unwrap();
                    deletions.borrow_mut().push(id);
                    vbox_exchanges.remove(&child);
                },
                VecDiff::Clear {} => {
                    while let Some(child) = vbox_exchanges.first_child() {
                        if child == prompt_text_box {
                            break;
                        }

                        vbox_exchanges.remove(&child);
                    }
                },
                _ => panic!("Not supported: {:?}", vd)
            }
            async {}
        }
    }));

    return (prompt_text_box.buffer(), vbox_exchanges);
}

pub fn Chat() -> impl IsA<gtk::Widget> {
    let exchanges: MutableVec<(String, String)> = MutableVec::new();
    let response_tokens = MutableVec::new();
    let streaming = Mutable::new(false);
    let edit_mode = Mutable::new(false);
    let clear_prompt = Rc::new(Notify::new());

    let (prompt_buffer, vbox_exchanges) = Exchanges(
        exchanges.clone(),
        response_tokens.clone(),
        streaming.clone(),
        edit_mode.clone(),
        clear_prompt.clone()
    );

    let scrolled_window = gtk::ScrolledWindow::new();
    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&vbox_exchanges));
    scrolled_window.set_vexpand(true);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 5);

    hbox.append(&NewButton(exchanges.clone(), streaming.clone(), clear_prompt.clone()));

    hbox.append(&SubmitButton(
        exchanges,
        move || get_buffer_content(&prompt_buffer),
        clear_prompt,
        response_tokens,
        streaming.clone()
    ));

    let dummy_label = gtk::Label::new(None);
    dummy_label.set_hexpand(true);
    hbox.append(&dummy_label);

    hbox.append(&EditModeButton(edit_mode, streaming.clone()));

    let cancel_button = CancelButton(streaming);
    hbox.append(&cancel_button);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    vbox.set_css_classes(&["top-level-box"]);
    vbox.append(&scrolled_window);
    vbox.append(&hbox);
    
    return vbox;
}
