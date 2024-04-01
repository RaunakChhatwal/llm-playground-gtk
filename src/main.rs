#![allow(non_snake_case)]
use gtk::{gdk, prelude::*};

mod chat;
use crate::chat::Chat;


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
        let context = gdk::Display::app_launch_context(&gdk::Display::default().expect("Could not connect to a display."));
        context.set_icon(Some(&pixbuf));

        App(app);
    });

    application.run();
    return Ok(());
}

fn App(application: &gtk::Application) {
    let window = gtk::ApplicationWindow::new(application);
    window.set_default_size(500, 600);

    window.set_title(Some("Chat Playground"));

    let chat = Chat();

    window.set_child(Some(&chat));

    application.connect_activate(move |_| {
        window.present();
    });
}