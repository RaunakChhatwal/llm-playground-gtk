#![allow(non_snake_case)]
use futures_signals::signal::SignalExt;
use gtk::{gdk, glib, prelude::*};
use settings::{load_settings, save_settings};

mod util;

mod settings;

mod submit;

mod chat;
use crate::chat::Chat;

mod settings_menu;
use crate::settings_menu::SettingsMenu;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

        let icon_bytes = include_bytes!("icon.ico");
        let pixbuf_loader = gtk::gdk_pixbuf::PixbufLoader::new();
        pixbuf_loader.write(icon_bytes).unwrap();
        pixbuf_loader.close().unwrap();
        let pixbuf = pixbuf_loader.pixbuf().unwrap();
        let context = gdk::Display::app_launch_context(&gdk::Display::default()
            .expect("Could not connect to a display."));
        context.set_icon(Some(&pixbuf));

        App(app);
    });

    application.run();
    return Ok(());
}

fn App(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_default_size(500, 650);

    window.set_title(Some("Chat Playground"));

    let settings = load_settings();

    let stack = gtk::Stack::new();

    let chat = Chat(stack.clone(), settings.clone());
    let settings_page = SettingsMenu(stack.clone(), settings.clone());

    stack.add_titled(&chat, Some("chat"), "Chat");
    stack.add_titled(&settings_page, Some("settings"), "Settings");
    stack.set_visible_child_name("chat");

    window.set_child(Some(&stack));

    glib::spawn_future_local(settings.signal_cloned().for_each(move |settings| async move {
        save_settings(&settings);
    }));

    application.connect_activate(move |_| {
        window.present();
    });
}