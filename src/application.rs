use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk4::{gdk, gio, glib};

use crate::config;
use crate::window::CamOverlayWindow;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct CamOverlayApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for CamOverlayApplication {
        const NAME: &'static str = "CamOverlayApplication";
        type Type = super::CamOverlayApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for CamOverlayApplication {}

    impl ApplicationImpl for CamOverlayApplication {
        fn activate(&self) {
            self.parent_activate();
            let app = self.obj();

            let window = CamOverlayWindow::new(&*app);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();
            let app = self.obj();

            app.setup_css();
            app.setup_about_action();
        }
    }

    impl GtkApplicationImpl for CamOverlayApplication {}
    impl AdwApplicationImpl for CamOverlayApplication {}
}

glib::wrapper! {
    pub struct CamOverlayApplication(ObjectSubclass<imp::CamOverlayApplication>)
        @extends adw::Application, gtk4::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl CamOverlayApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            .property("flags", gio::ApplicationFlags::FLAGS_NONE)
            .build()
    }

    fn setup_css(&self) {
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(include_str!("../data/style.css"));
        gtk4::style_context_add_provider_for_display(
            &gdk::Display::default().expect("Could not get display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
    }

    fn setup_about_action(&self) {
        let action = gio::SimpleAction::new("about", None);
        let app = self.clone();
        action.connect_activate(move |_, _| {
            let window = app
                .active_window()
                .unwrap_or_else(|| panic!("No active window"));

            let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
            content.set_margin_top(24);
            content.set_margin_bottom(24);
            content.set_margin_start(24);
            content.set_margin_end(24);

            let icon = gtk4::Image::from_icon_name(config::APP_ID);
            icon.set_pixel_size(64);
            content.append(&icon);

            let heading = gtk4::Label::new(Some(&format!("Cam Overlay  v{}", config::VERSION)));
            heading.add_css_class("title-1");
            content.append(&heading);

            let body = gtk4::Label::new(None);
            body.set_markup("Webcam preview overlay for screen recording.\n\nCreated for fun by @didley, with love from Melbourne.\n\n<b>Always on Top</b>\nUse your compositor's window menu to set always on top.\nOn GNOME: Super + Right Click on the window, or Alt + Space.\n\n<b>Full Screen</b>\nDouble left-click the overlay to toggle full screen.\n\nLicense: GPL-3.0-or-later\n<a href=\"https://github.com/didley/camoverlay\">github.com/didley/camoverlay</a>");
            body.set_xalign(0.0);
            body.set_wrap(true);
            content.append(&body);

            let toolbar_view = adw::ToolbarView::new();
            let header = adw::HeaderBar::new();
            header.set_show_title(false);
            toolbar_view.add_top_bar(&header);

            let scroll = gtk4::ScrolledWindow::new();
            scroll.set_child(Some(&content));
            scroll.set_propagate_natural_height(true);
            toolbar_view.set_content(Some(&scroll));

            let about = adw::Dialog::builder()
                .title("About")
                .child(&toolbar_view)
                .content_width(450)
                .content_height(620)
                .build();

            about.present(Some(&window));
        });
        self.add_action(&action);
    }
}
