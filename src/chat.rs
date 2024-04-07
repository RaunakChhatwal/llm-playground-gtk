#![allow(non_snake_case)]
use std::{cell::RefCell, rc::Rc};

use gtk::{glib::{self, clone}, prelude::*, Label};
use futures_signals::{signal::{Mutable, SignalExt}, signal_vec::{MutableVec, SignalVecExt, VecDiff}};
use tokio::sync::Notify;

use crate::{settings::Settings, submit::SubmitButton, util::{get_buffer_content, DummyLabel}};


fn MessageTextBox(message: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(message));
    label.set_css_classes(&["message-label"]);
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

fn NewButton(
    exchanges: MutableVec<(String, String)>,
    streaming: Mutable<bool>,
    clear_prompt: Rc<Notify>
) -> impl IsA<gtk::Widget> {
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
    edit_exchange: impl Fn((String, String)) + 'static,
    delete_exchange: impl Fn() + 'static
) -> ExchangeWidget {
    let exchange = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let overlay = gtk::Overlay::new();

    let user_text_box = MessageTextBox(&user_message);
    user_text_box.set_valign(gtk::Align::Start);
    user_text_box.set_hexpand(true);
    let editable_user_text_box = EditableMessageTextBox(&user_message);
    editable_user_text_box.set_hexpand(true);
    overlay.set_child(Some(&user_text_box));

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_css_classes(&["button-box"]);
    hbox.set_halign(gtk::Align::End);
    hbox.set_valign(gtk::Align::Start);
    let edit_button = HeaderOption("Edit");
    hbox.append(&edit_button);

    let delete_button = HeaderOption("Delete");
    delete_button.set_valign(gtk::Align::Start);
    delete_button.connect_clicked(move |_| delete_exchange());
    hbox.append(&delete_button);

    let done_button = HeaderOption("Done");
    done_button.set_visible(false);
    hbox.append(&done_button);

    overlay.add_overlay(&hbox);

    exchange.append(&overlay);

    let assistant_text_box = MessageTextBox(&assistant_message);
    let editable_assistant_text_box = EditableMessageTextBox(&assistant_message);
    exchange.append(&assistant_text_box);
    exchange.set_hexpand(true);

    edit_button.connect_clicked(clone!(
        @weak edit_button,
        @weak delete_button,
        @weak done_button,
        @weak overlay,
        @weak exchange,
        @strong user_text_box,
        @strong assistant_text_box,
        @strong editable_user_text_box,
        @strong editable_assistant_text_box
        => move |_| {
            edit_button.set_visible(false);
            delete_button.set_visible(false);
            exchange.remove(&assistant_text_box);
            editable_user_text_box.buffer().set_text(&user_text_box.label().to_string());
            editable_assistant_text_box.buffer().set_text(&assistant_text_box.label().to_string());
            overlay.set_child(Some(&editable_user_text_box));
            exchange.append(&editable_assistant_text_box);
            done_button.set_visible(true);
        }
    ));

    done_button.connect_clicked(clone!(
        @weak done_button,
        @weak exchange
        => move |_| {
            done_button.set_visible(false);
            exchange.remove(&editable_assistant_text_box);
            user_text_box.set_label(&get_buffer_content(&editable_user_text_box.buffer()));
            assistant_text_box.set_label(&get_buffer_content(&editable_assistant_text_box.buffer()));
            edit_exchange((user_text_box.label().to_string(), assistant_text_box.label().to_string()));
            overlay.set_child(Some(&user_text_box));
            exchange.append(&assistant_text_box);
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
    clear_prompt: Rc<Notify>
) -> (gtk::TextBuffer, gtk::Box) {
    let id_counter = Rc::new(RefCell::new(0usize));
    let deletions: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
    let exchanges_memo: Rc<RefCell<Vec<ExchangeWidget>>> = Rc::new(RefCell::new(vec![]));

    let vbox_exchanges = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let prompt_text_box = PromptTextBox(clear_prompt.clone());
    let response_text_box = ResponseTextBox(&response_tokens, streaming.clone());

    vbox_exchanges.append(&prompt_text_box);
    vbox_exchanges.append(&response_text_box);

    glib::spawn_future_local(exchanges.signal_vec_cloned().for_each({
        let vbox_exchanges = vbox_exchanges.clone();
        let prompt_text_box = prompt_text_box.clone();
        let exchanges = exchanges.clone();
        move |vd| {
            match vd {
                VecDiff::UpdateAt { index: _, value: _ } => {},
                VecDiff::Push { value: (user_message, assistant_message) } => {
                    let id = *(*id_counter).borrow();
                    *id_counter.borrow_mut() += 1;
                    let exchange = Exchange(user_message, assistant_message, {
                        let exchanges = exchanges.clone();
                        let deletions = deletions.clone();
                        move |new_exchange| edit_exchange(&exchanges, new_exchange, &deletions, id)
                    }, {
                        let exchanges = exchanges.clone();
                        let deletions = deletions.clone();
                        move || delete_exchange(&exchanges, &deletions, id)
                    });
                    exchange.insert_before(&vbox_exchanges, Some(&prompt_text_box));
                    exchanges_memo.borrow_mut().push(exchange);
                },
                VecDiff::RemoveAt { index } => {
                    let child = exchanges_memo.borrow_mut().remove(index);
                    deletions.borrow_mut().push(index);
                    vbox_exchanges.remove(&child);
                },
                VecDiff::Pop {} => {
                    let child = exchanges_memo.borrow_mut().pop().unwrap();
                    deletions.borrow_mut().push(exchanges_memo.borrow().len());
                    vbox_exchanges.remove(&child);
                },
                VecDiff::Clear {} => {
                    for exchange in exchanges_memo.borrow().iter() {
                        vbox_exchanges.remove(exchange);
                    }
                    *id_counter.borrow_mut() = 0;
                    deletions.borrow_mut().clear();
                    exchanges_memo.borrow_mut().clear();
                },
                _ => panic!("Not supported: {:?}", vd)
            }
            async {}
        }
    }));

    return (prompt_text_box.buffer(), vbox_exchanges);
}

fn SettingsButton(stack: gtk::Stack) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_label("Settings");

    button.connect_clicked(move |_| stack.set_visible_child_name("settings"));
    return button;
}

fn ErrorLabel(error: Mutable<String>) -> Label {
    let label = Label::new(Some(""));
    label.set_css_classes(&["error-label"]);

    glib::spawn_future_local(error.signal_cloned().for_each({
        let label = label.clone();
        move |error| {
            label.set_text(&error);
            label.set_visible(&error != "");
            async {}
        }
    }));

    return label;
}

pub fn Chat(stack: gtk::Stack, settings: Mutable<Settings>) -> impl IsA<gtk::Widget> {
    let exchanges: MutableVec<(String, String)> = MutableVec::new();
    let response_tokens = MutableVec::new();
    let streaming = Mutable::new(false);
    let error = Mutable::new(String::new());
    let clear_prompt = Rc::new(Notify::new());

    let (prompt_buffer, vbox_exchanges) = Exchanges(
        exchanges.clone(),
        response_tokens.clone(),
        streaming.clone(),
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
        settings,
        clear_prompt,
        response_tokens,
        error.clone(),
        streaming.clone()
    ));

    hbox.append(&DummyLabel(gtk::Orientation::Horizontal));

    let cancel_button = CancelButton(streaming.clone());
    hbox.append(&cancel_button);

    hbox.append(&SettingsButton(stack));

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    vbox.set_css_classes(&["top-level-box"]);
    vbox.append(&ErrorLabel(error));
    vbox.append(&scrolled_window);
    vbox.append(&hbox);
    
    return vbox;
}
