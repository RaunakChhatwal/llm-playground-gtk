#![allow(non_snake_case)]
use gtk::{gdk, glib, prelude::*};
use futures_signals::signal::{Mutable, SignalExt};

#[tokio::main]
async fn main() -> glib::ExitCode {
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

    application.run()
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
    let text_view = gtk::TextView::new();
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    let scrolled_window = gtk::ScrolledWindow::new();
    scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled_window.set_child(Some(&text_view));
    scrolled_window.set_height_request(75);

    glib::spawn_future_local(response.signal_cloned().for_each(move |response| {
        text_view.clone().buffer().set_text(&response);
        async {}
    }));
    return scrolled_window;
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
        let mut response = response.lock_mut();
        let (start, end) = &prompt_buffer.bounds();
        *response = prompt_buffer.text(start, end, false).to_string();
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