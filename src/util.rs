use gtk::{prelude::*, Label};

pub fn get_buffer_content(buffer: &gtk::TextBuffer) -> String {
    let (start, end) = &buffer.bounds();
    return buffer.text(start, end, false).to_string();
}

pub fn DummyLabel(orientation: gtk::Orientation) -> Label {
    let label = Label::new(None);
    match orientation {
        gtk::Orientation::Horizontal => label.set_hexpand(true),
        gtk::Orientation::Vertical => label.set_vexpand(true),
        _ => panic!("Not supported")
    }

    return label;
}

pub fn center(widget: impl IsA<gtk::Widget>) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.append(&DummyLabel(gtk::Orientation::Horizontal));
    hbox.append(&widget);
    hbox.append(&DummyLabel(gtk::Orientation::Horizontal));

    return hbox;
}