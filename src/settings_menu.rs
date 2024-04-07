use futures::{channel::mpsc, StreamExt};
use futures_signals::signal::{Mutable, SignalExt};
use gtk::{glib::{self, clone}, prelude::*, Button, DropDown, Entry, Label, Scale, ScrolledWindow, Window};
use maplit::hashmap;

use crate::{settings::{APIKey, Provider, Settings}, util::{center, DummyLabel}};

fn TemperatureSlider(
    temperature: Mutable<f64>,
    mut temperature_recv: mpsc::UnboundedReceiver<f64>,
    changes_made: Mutable<bool>
) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 20);

    let label = Label::new(Some("Temperature:"));
    hbox.append(&label);

    // hbox_slider's components aren't directly appended to hbox
    // because I don't want 20px margin to be applied between them
    let hbox_slider = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox_slider.append(&Label::new(Some("0")));
    let adjustment = gtk::Adjustment::new(*temperature.lock_ref(), 0.0, 1.0, 0.1, 0.1, 0.0);
    let slider = Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    slider.set_digits(1);
    slider.set_width_request(150);
    slider.set_valign(gtk::Align::Center);
    hbox_slider.append(&slider);
    hbox_slider.append(&Label::new(Some("1")));
    hbox.append(&hbox_slider);

    hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    let value_label = Label::new(Some("1.0"));
    hbox.append(&value_label);

    adjustment.connect_value_changed(clone!(
        @strong temperature => move |adjustment| {
            value_label.set_text(&format!("{:.1}", adjustment.value()));
            *temperature.lock_mut() = adjustment.value();
            *changes_made.lock_mut() = true;
    }));

    glib::spawn_future_local(async move {
        while let Some(temperature) = temperature_recv.next().await {
            adjustment.set_value(temperature);
        }
    });

    return hbox;
}

fn MaxTokensEntry(
    max_tokens: Mutable<u32>,
    mut max_tokens_recv: mpsc::UnboundedReceiver<u32>,
    changes_made: Mutable<bool>
) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 20);

    let label = Label::new(Some("Max. tokens:"));
    hbox.append(&label);

    let entry = Entry::new();
    entry.set_text(&max_tokens.lock_ref().to_string());
    hbox.append(&entry);

    entry.connect_changed(clone!(@strong max_tokens => move |entry| {
        if let Ok(value) = entry.text().trim().parse() {
            *max_tokens.lock_mut() = value;
            *changes_made.lock_mut() = true;
        }
    }));

    glib::spawn_future_local(async move {
        while let Some(max_tokens) = max_tokens_recv.next().await {
            entry.set_text(&max_tokens.to_string());
        }
    });

    return hbox;
}

fn ModelEntry(
    model: Mutable<String>,
    mut model_recv: mpsc::UnboundedReceiver<String>,
    changes_made: Mutable<bool>
) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 20);

    let label = Label::new(Some("Model:"));
    hbox.append(&label);

    let entry = Entry::new();
    entry.set_text(&model.lock_ref().as_ref());
    hbox.append(&entry);

    entry.connect_changed(clone!(
        @strong model => move |entry| {
            model.lock_mut().clone_from(&entry.text().to_string());
            *changes_made.lock_mut() = true;
        }
    ));

    glib::spawn_future_local(async move {
        while let Some(max_tokens) = model_recv.next().await {
            entry.set_text(&max_tokens.to_string());
        }
    });

    return hbox;
}

fn APIKeyDropDown(
    api_key: Mutable<Option<usize>>,
    mut api_key_recv: mpsc::UnboundedReceiver<Option<usize>>,
    api_keys: Mutable<Vec<APIKey>>,
    changes_made: Mutable<bool>
) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 20);

    let label = Label::new(Some("API key:"));
    hbox.append(&label);

    let api_keys_vec = api_keys.lock_ref().clone();
    let key_names: Vec<&str> = api_keys_vec
        .iter()
        .map(|api_key|
            api_key.name.as_str())
        .collect();
    let store = gtk::StringList::new(&key_names);
    let dropdown = DropDown::new(Some(store), None::<&gtk::Expression>);
    hbox.append(&dropdown);

    dropdown.connect_notify(Some("selected"), clone!(@strong api_key, @strong api_keys =>
        move |drop_down, _| {
            if api_keys.lock_ref().is_empty() {
                *api_key.lock_mut() = None;
            } else {
                *api_key.lock_mut() = Some(drop_down.selected() as usize);
            }
            *changes_made.lock_mut() = true;
        }
    ));

    glib::spawn_future_local(api_keys.signal_cloned().for_each({
        let dropdown = dropdown.clone();
        move |api_keys| {
            let key_names: Vec<&str> = api_keys
                .iter()
                .map(|api_key|
                    api_key.name.as_str())
                .collect();
            let store = gtk::StringList::new(&key_names);
            dropdown.set_model(Some(&store));
            async {}
        }
    }));

    glib::spawn_future_local(clone!(@weak dropdown => async move {
        while let Some(api_key) = api_key_recv.next().await {
            if let Some(api_key) = api_key {
                dropdown.set_selected(api_key as u32);
            }
        }
    }));

    return hbox;
}

fn Popup(api_keys: Mutable<Vec<APIKey>>) -> Window {
    let popup_window = Window::new();
    popup_window.set_css_classes(&["popup-window"]);
    popup_window.set_title(Some("Add API Key"));
    popup_window.set_modal(true);
    popup_window.set_resizable(false);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let error_label = Label::new(None);
    error_label.set_css_classes(&["error-label"]);
    error_label.set_visible(false);
    vbox.append(&error_label);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(10);
    grid.set_column_spacing(10);

    let label = Label::new(Some("Name: "));
    label.set_halign(gtk::Align::Start);
    grid.attach(&label, 0, 0, 1, 1);
    let name_entry = Entry::new();
    grid.attach(&name_entry, 1, 0, 1, 1);

    let label = Label::new(Some("Key: "));
    label.set_halign(gtk::Align::Start);
    grid.attach(&label, 0, 1, 1, 1);
    let key_entry = Entry::new();
    grid.attach(&key_entry, 1, 1, 1, 1);

    let label = Label::new(Some("Provider: "));
    label.set_halign(gtk::Align::Start);
    grid.attach(&label, 0, 2, 1, 1);
    let providers = hashmap! {
        "OpenAI" => Provider::OpenAI,
        "Anthropic" => Provider::Anthropic,
    };
    let provider_names: Vec<&str> = providers.keys().map(|x| *x).collect();
    let store = gtk::StringList::new(&provider_names);
    let provider_dropdown = DropDown::new(Some(store), None::<&gtk::Expression>);
    grid.attach(&provider_dropdown, 1, 2, 1, 1);

    vbox.append(&grid);

    let add_button = Button::new();
    add_button.set_halign(gtk::Align::End);
    add_button.set_label("Add");
    add_button.connect_clicked(clone!(
        @weak popup_window,
        @strong api_keys => move |_| {
            let name = name_entry.text().to_string();
            let key = key_entry.text().to_string();
            let provider_name = provider_names[provider_dropdown.selected() as usize];
            let provider = *providers.get(&provider_name).unwrap();
            if api_keys.lock_ref().iter().any(|k| k.name == name) {
                error_label.set_label("API key name already exists");
                error_label.set_visible(true);
            } else {
                api_keys.lock_mut().push(APIKey {
                    name,
                    key,
                    provider,
                });
                popup_window.close();
            }
        }
    ));

    vbox.append(&add_button);

    popup_window.set_child(Some(&vbox));
    return popup_window;
}

fn APIKeyList(api_keys: Mutable<Vec<APIKey>>, changes_made: Mutable<bool>) -> ScrolledWindow {
    let scrolled_window = ScrolledWindow::new();
    scrolled_window.set_vexpand(true);
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let api_keys_title = Label::new(Some("API Keys"));
    api_keys_title.set_halign(gtk::Align::Start);
    vbox.append(&api_keys_title);

    let listbox = gtk::Box::new(gtk::Orientation::Vertical, 5);
    listbox.set_css_classes(&["api-key-list"]);
    vbox.append(&listbox);

    let add_key_button = Button::new();
    add_key_button.set_label("+");
    add_key_button.set_halign(gtk::Align::Start);
    vbox.append(&add_key_button);

    add_key_button.connect_clicked(clone!(@strong api_keys => move |_| {
        let popup_window = Popup(api_keys.clone());
        popup_window.present();
    }));

    glib::spawn_future_local(api_keys.signal_cloned().for_each(move |api_keys_vec| {
        while let Some(child) = listbox.first_child() {
            listbox.remove(&child);
        }
        for api_key in api_keys_vec {
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            hbox.append(&Label::new(Some(&api_key.name)));
            hbox.append(&Label::new(Some(&api_key.provider.to_string())));

            let delete_button = Button::new();
            delete_button.set_css_classes(&["delete-button"]);
            delete_button.set_label("-");
            delete_button.connect_clicked(clone!(@strong api_keys => move |_| {
                let mut lock = api_keys.lock_mut();
                let index = lock.iter().position(|key| key.name == api_key.name).unwrap();
                lock.remove(index);
            }));
            hbox.append(&delete_button);

            listbox.append(&hbox);
        }
        *changes_made.lock_mut() = true;
        async {}
    }));

    scrolled_window.set_child(Some(&vbox));
    return scrolled_window;
}

fn SettingsBody(settings: Mutable<Settings>) -> gtk::Box {
    let temperature = Mutable::new(settings.lock_ref().temperature);
    let (temperature_send, temperature_recv) = mpsc::unbounded();

    let max_tokens = Mutable::new(settings.lock_ref().max_tokens);
    let (max_tokens_send, max_tokens_recv) = mpsc::unbounded();

    let model = Mutable::new(settings.lock_ref().model.clone());
    let (model_send, model_recv) = mpsc::unbounded();

    let api_key = Mutable::new(settings.lock_ref().api_key.clone());
    let (api_key_send, api_key_recv) = mpsc::unbounded();

    let api_keys: Mutable<Vec<APIKey>> = Mutable::new(settings.lock_ref().api_keys.clone());

    let changes_made = Mutable::new(false);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 15);
    vbox.append(&TemperatureSlider(temperature.clone(), temperature_recv, changes_made.clone()));
    vbox.append(&MaxTokensEntry(max_tokens.clone(), max_tokens_recv, changes_made.clone()));
    vbox.append(&ModelEntry(model.clone(), model_recv, changes_made.clone()));
    vbox.append(&APIKeyDropDown(api_key.clone(), api_key_recv, api_keys.clone(), changes_made.clone()));

    vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    vbox.append(&APIKeyList(api_keys.clone(), changes_made.clone()));

    // work-around for bug where the drop-down doesn't correcty register the selected api key
    glib::spawn_future_local(clone!(@strong api_key_send, @strong settings => async move {
        api_key_send.unbounded_send(settings.lock_ref().api_key.clone()).unwrap();
    }));

    glib::spawn_future_local(clone!(@strong changes_made => async move {
        *changes_made.lock_mut() = false;
    }));

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    hbox.set_halign(gtk::Align::End);
    hbox.set_visible(false);
    let discard_button = Button::new();
    discard_button.set_label("Discard");
    hbox.append(&discard_button);
    let apply_button = Button::new();
    apply_button.set_label("Apply");
    hbox.append(&apply_button);
    vbox.append(&hbox);

    discard_button.connect_clicked(clone!(
        @strong api_keys,
        @strong changes_made,
        @strong settings => move |_| {
            temperature_send.unbounded_send(settings.lock_ref().temperature).unwrap();
            max_tokens_send.unbounded_send(settings.lock_ref().max_tokens).unwrap();
            model_send.unbounded_send(settings.lock_ref().model.clone()).unwrap();
            api_key_send.unbounded_send(settings.lock_ref().api_key.clone()).unwrap();
            api_keys.lock_mut().clone_from(&settings.lock_ref().api_keys);
            let changes_made = changes_made.clone();
            glib::spawn_future_local(async move { *changes_made.lock_mut() = false; });
        }
    ));

    apply_button.connect_clicked(clone!(
        @weak hbox,
        @strong changes_made => move |_| {
            *changes_made.lock_mut() = false;
            *settings.lock_mut() = Settings {
                temperature: *temperature.lock_ref(),
                max_tokens: *max_tokens.lock_ref(),
                model: model.lock_ref().clone(),
                api_key: *api_key.lock_ref(),
                api_keys: api_keys.lock_ref().clone()
            };
        }
    ));

    glib::spawn_future_local(changes_made.signal().for_each(move |changes_made| {
        hbox.set_visible(changes_made);
        async {}
    }));

    return vbox;
}

pub fn SettingsMenu(stack: gtk::Stack, settings: Mutable<Settings>) -> gtk::Box {
    let vbox_settings = gtk::Box::new(gtk::Orientation::Vertical, 15);
    vbox_settings.set_css_classes(&["settings-box", "top-level-box"]);
    let back_button = Button::new();
    back_button.set_label("Back");
    back_button.set_halign(gtk::Align::Start);
    back_button.connect_clicked(move |_| stack.set_visible_child_name("chat"));
    vbox_settings.append(&back_button);

    let title = Label::new(Some("Settings"));
    title.set_css_classes(&["title"]);
    vbox_settings.append(&title);

    vbox_settings.append(&DummyLabel(gtk::Orientation::Vertical));

    vbox_settings.append(&center(SettingsBody(settings)));

    vbox_settings.append(&DummyLabel(gtk::Orientation::Vertical));

    return vbox_settings;
}