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

            let about = adw::AboutDialog::builder()
                .application_name("Cam Overlay")
                .version(config::VERSION)
                .application_icon(config::APP_ID)
                .license_type(gtk4::License::Gpl30)
                .comments("Webcam preview overlay for screen recording.\n\nTip: Use your compositor's window menu (e.g. Super+Right Click) to set Always on Top.")
                .build();

            about.present(Some(&window));
        });
        self.add_action(&action);
    }
}
