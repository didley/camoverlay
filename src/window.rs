use adw::prelude::*;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use glib::prelude::IsA;
use gtk4::{gdk, gio, glib};
use gstreamer::prelude::*;
use std::cell::{Cell, RefCell};

const SETTINGS_SCHEMA: &str = "io.github.didley.CamOverlay";

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct CamOverlayWindow {
        pub pipeline: RefCell<Option<gstreamer::Element>>,
        pub settings: RefCell<Option<gio::Settings>>,
        pub overlay_container: RefCell<Option<gtk4::Overlay>>,
        pub video_picture: RefCell<Option<gtk4::Picture>>,
        pub is_expanded: Cell<bool>,
        pub compact_width: Cell<i32>,
        pub compact_height: Cell<i32>,
        pub video_width: Cell<i32>,
        pub video_height: Cell<i32>,
        pub cursor_edge: RefCell<Option<gdk::SurfaceEdge>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CamOverlayWindow {
        const NAME: &'static str = "CamOverlayWindow";
        type Type = super::CamOverlayWindow;
        type ParentType = gtk4::ApplicationWindow;
    }

    impl ObjectImpl for CamOverlayWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().init_window();
        }

        fn dispose(&self) {
            if let Some(pipeline) = self.pipeline.borrow().as_ref() {
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
        }
    }

    impl WidgetImpl for CamOverlayWindow {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let overlay_ref = self.overlay_container.borrow();
            let overlay = overlay_ref.as_ref();
            let expanded = self.is_expanded.get();

            let is_circle = !expanded && overlay.map(|o| o.has_css_class("circle")).unwrap_or(false);
            let is_rounded = !expanded && overlay.map(|o| o.has_css_class("rounded-rect")).unwrap_or(false);

            let w = widget.width() as f32;
            let h = widget.height() as f32;
            let border_color = [gdk::RGBA::new(0.0, 0.0, 0.0, 0.4); 4];
            let border_width = [2.0f32; 4];

            if is_circle {
                let size = w.min(h);
                let x = (w - size) / 2.0;
                let y = (h - size) / 2.0;
                let rounded = gtk4::gsk::RoundedRect::from_rect(
                    gtk4::graphene::Rect::new(x, y, size, size),
                    size / 2.0,
                );
                snapshot.push_rounded_clip(&rounded);
                self.parent_snapshot(snapshot);
                snapshot.append_border(&rounded, &border_width, &border_color);
                snapshot.pop();
            } else if is_rounded {
                let rounded = gtk4::gsk::RoundedRect::from_rect(
                    gtk4::graphene::Rect::new(0.0, 0.0, w, h),
                    16.0,
                );
                snapshot.push_rounded_clip(&rounded);
                self.parent_snapshot(snapshot);
                snapshot.append_border(&rounded, &border_width, &border_color);
                snapshot.pop();
            } else {
                self.parent_snapshot(snapshot);
            }
        }

        fn contains(&self, x: f64, y: f64) -> bool {
            let widget = self.obj();
            let is_circle = !self.is_expanded.get()
                && self.overlay_container
                    .borrow()
                    .as_ref()
                    .map(|o| o.has_css_class("circle"))
                    .unwrap_or(false);

            if is_circle {
                let w = widget.width() as f64;
                let h = widget.height() as f64;
                let cx = w / 2.0;
                let cy = h / 2.0;
                let dx = x - cx;
                let dy = y - cy;
                // Circle interior + thin ring outside the edge for resize handle grab
                let grab_r = w.min(h) / 2.0 + 8.0;
                dx * dx + dy * dy <= grab_r * grab_r
            } else {
                self.parent_contains(x, y)
            }
        }
    }
    impl WindowImpl for CamOverlayWindow {}
    impl ApplicationWindowImpl for CamOverlayWindow {}
}

glib::wrapper! {
    pub struct CamOverlayWindow(ObjectSubclass<imp::CamOverlayWindow>)
        @extends gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl CamOverlayWindow {
    pub fn new(app: &impl IsA<gtk4::Application>) -> Self {
        glib::Object::builder()
            .property("application", app)
            .build()
    }

    fn init_window(&self) {
        let imp = self.imp();

        let settings = gio::Settings::new(SETTINGS_SCHEMA);
        *imp.settings.borrow_mut() = Some(settings.clone());

        self.add_css_class("cam-overlay");
        self.set_decorated(false);
        self.set_resizable(true);
        self.set_size_request(60, 60);

        let saved_width = settings.int("window-width");
        let saved_height = settings.int("window-height");
        let shape = settings.string("shape");
        let (init_w, init_h) = if shape.as_str() == "circle" {
            let s = saved_width.min(saved_height).max(1);
            (s, s)
        } else {
            (saved_width, saved_height)
        };
        self.set_default_size(init_w, init_h);
        imp.compact_width.set(init_w);
        imp.compact_height.set(init_h);

        let overlay_container = gtk4::Overlay::new();
        let video_picture = gtk4::Picture::new();

        let fit_mode = settings.string("fit-mode");
        let fit = match fit_mode.as_str() {
            "fill" => gtk4::ContentFit::Fill,
            _      => gtk4::ContentFit::Cover,
        };
        video_picture.set_content_fit(fit);

        overlay_container.set_child(Some(&video_picture));
        self.set_child(Some(&overlay_container));

        *imp.overlay_container.borrow_mut() = Some(overlay_container.clone());
        *imp.video_picture.borrow_mut() = Some(video_picture);

        overlay_container.add_css_class(shape.as_str());

        self.setup_pipeline();
        self.setup_drag();
        self.setup_motion();
        self.setup_double_click();
        self.setup_context_menu();
        self.setup_actions();

        // Save compact size on resize; enforce square in circle mode
        let win = self.clone();
        self.connect_notify_local(Some("default-width"), move |_, _| {
            if !win.imp().is_expanded.get() {
                let mut w = win.default_width();
                let mut h = win.default_height();
                if w > 0 && h > 0 {
                    let is_circle = win.imp().settings.borrow().as_ref()
                        .map(|s| s.string("shape") == "circle")
                        .unwrap_or(false);
                    if is_circle {
                        let size = w.min(h);
                        w = size;
                        h = size;
                        win.set_default_size(size, size);
                    }
                    win.imp().compact_width.set(w);
                    win.imp().compact_height.set(h);
                    if let Some(s) = win.imp().settings.borrow().as_ref() {
                        let _ = s.set_int("window-width", w);
                        let _ = s.set_int("window-height", h);
                    }
                }
            }
            win.update_input_region();
        });
    }

    fn setup_pipeline(&self) {
        let imp = self.imp();

        let pipeline = match gstreamer::parse::launch(
            "pipewiresrc ! videoconvert name=converter ! videoflip name=flipper method=none ! videocrop name=cropper ! videoscale ! gtk4paintablesink name=sink sync=false"
        ) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to create pipeline: {e}");
                return;
            }
        };

        let sink = pipeline.downcast_ref::<gstreamer::Bin>()
            .and_then(|bin| bin.by_name("sink"))
            .expect("Sink element not found");

        let paintable = sink.property::<gdk::Paintable>("paintable");

        if let Some(picture) = imp.video_picture.borrow().as_ref() {
            picture.set_paintable(Some(&paintable));
        }

        // Apply saved flip
        let flipped = imp.settings.borrow().as_ref()
            .map(|s| s.boolean("flipped"))
            .unwrap_or(false);
        if flipped {
            if let Some(bin) = pipeline.downcast_ref::<gstreamer::Bin>() {
                if let Some(flipper) = bin.by_name("flipper") {
                    flipper.set_property_from_str("method", "horizontal-flip");
                }
            }
        }

        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
            eprintln!("Failed to start pipeline: {e}");
        }

        *imp.pipeline.borrow_mut() = Some(pipeline);

        // Poll for negotiated caps on the main thread, then apply saved zoom + fit
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let imp = win.imp();
            let pipeline_ref = imp.pipeline.borrow();
            let Some(pipeline) = pipeline_ref.as_ref() else {
                return glib::ControlFlow::Break;
            };
            let Some(bin) = pipeline.downcast_ref::<gstreamer::Bin>() else {
                return glib::ControlFlow::Break;
            };
            let Some(converter) = bin.by_name("converter") else {
                return glib::ControlFlow::Break;
            };
            let Some(src_pad) = converter.static_pad("src") else {
                return glib::ControlFlow::Break;
            };
            let Some(caps) = src_pad.current_caps() else {
                return glib::ControlFlow::Continue;
            };
            let Some(s) = caps.structure(0) else {
                return glib::ControlFlow::Continue;
            };
            let width = s.get::<i32>("width").unwrap_or(0);
            let height = s.get::<i32>("height").unwrap_or(0);
            if width > 0 && height > 0 {
                imp.video_width.set(width);
                imp.video_height.set(height);
                let zoom = imp.settings.borrow().as_ref()
                    .map(|s| s.int("zoom-level"))
                    .unwrap_or(1);
                let fit_mode = imp.settings.borrow().as_ref()
                    .map(|s| s.string("fit-mode").to_string())
                    .unwrap_or_default();
                drop(pipeline_ref);
                win.apply_zoom(zoom);
                win.apply_fit_mode(&fit_mode);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn setup_drag(&self) {
        let drag = gtk4::GestureDrag::new();
        drag.set_button(1);

        let initiated = std::rc::Rc::new(std::cell::Cell::new(false));
        let edge_at_press: std::rc::Rc<std::cell::Cell<Option<gdk::SurfaceEdge>>> =
            std::rc::Rc::new(std::cell::Cell::new(None));

        let win1 = self.clone();
        let initiated_begin = initiated.clone();
        let edge_begin = edge_at_press.clone();
        drag.connect_drag_begin(move |_, _, _| {
            initiated_begin.set(false);
            edge_begin.set(*win1.imp().cursor_edge.borrow());
        });

        let win = self.clone();
        drag.connect_drag_update(move |gesture, offset_x, offset_y| {
            if initiated.get() {
                return;
            }
            if (offset_x * offset_x + offset_y * offset_y) < 16.0 {
                return;
            }
            initiated.set(true);
            let edge = edge_at_press.get();
            use gtk4::prelude::NativeExt;
            let win_ref = win.upcast_ref::<gtk4::Window>();
            if let Some(surface) = win_ref.surface() {
                if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                    if let Some(device) = gesture.device() {
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                        if let Some(e) = edge {
                            toplevel.begin_resize(e, Some(&device), 1, 0.0, 0.0, gdk::CURRENT_TIME);
                        } else if !win.imp().is_expanded.get() {
                            toplevel.begin_move(&device, 1, 0.0, 0.0, gdk::CURRENT_TIME);
                        }
                    }
                }
            }
        });

        self.add_controller(drag);
    }

    fn setup_motion(&self) {
        const BORDER: f64 = 16.0;
        let motion = gtk4::EventControllerMotion::new();
        let win = self.clone();
        motion.connect_motion(move |_, x, y| {
            let imp = win.imp();
            // Use actual allocated size — default_width() lags during a WM-driven resize
            let w = win.width() as f64;
            let h = win.height() as f64;

            let is_circle = !imp.is_expanded.get()
                && imp.overlay_container.borrow().as_ref()
                    .map(|o| o.has_css_class("circle"))
                    .unwrap_or(false);

            let edge = if is_circle {
                let cx = w / 2.0;
                let cy = h / 2.0;
                let r = w.min(h) / 2.0;
                let dx = x - cx;
                let dy = y - cy;
                let dist_sq = dx * dx + dy * dy;
                let inner = r - BORDER;
                // Near the circle edge → corner resize based on quadrant so both
                // dimensions change together and the window stays square
                if dist_sq >= inner * inner {
                    Some(if x < cx {
                        if y < cy { gdk::SurfaceEdge::NorthWest } else { gdk::SurfaceEdge::SouthWest }
                    } else {
                        if y < cy { gdk::SurfaceEdge::NorthEast } else { gdk::SurfaceEdge::SouthEast }
                    })
                } else {
                    None // inside circle → move gesture
                }
            } else {
                let left   = x < BORDER;
                let right  = x > w - BORDER;
                let top    = y < BORDER;
                let bottom = y > h - BORDER;
                match (left, right, top, bottom) {
                    (true, _, true, _) => Some(gdk::SurfaceEdge::NorthWest),
                    (_, true, true, _) => Some(gdk::SurfaceEdge::NorthEast),
                    (true, _, _, true) => Some(gdk::SurfaceEdge::SouthWest),
                    (_, true, _, true) => Some(gdk::SurfaceEdge::SouthEast),
                    (true, _, _, _)    => Some(gdk::SurfaceEdge::West),
                    (_, true, _, _)    => Some(gdk::SurfaceEdge::East),
                    (_, _, true, _)    => Some(gdk::SurfaceEdge::North),
                    (_, _, _, true)    => Some(gdk::SurfaceEdge::South),
                    _                  => None,
                }
            };
            *imp.cursor_edge.borrow_mut() = edge;
        });
        self.add_controller(motion);
    }

    fn setup_double_click(&self) {
        let click = gtk4::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);

        let win = self.clone();
        click.connect_pressed(move |gesture, n_press, _, _| {
            if n_press == 2 {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                win.toggle_expanded();
            }
        });

        self.add_controller(click);
    }

    fn toggle_expanded(&self) {
        let imp = self.imp();
        let expanded = imp.is_expanded.get();

        if expanded {
            imp.is_expanded.set(false);
            self.unfullscreen();
            if let Some(overlay) = imp.overlay_container.borrow().as_ref() {
                let shape = imp.settings.borrow().as_ref()
                    .map(|s| s.string("shape").to_string())
                    .unwrap_or_else(|| "circle".to_string());
                overlay.add_css_class(&shape);
            }
            self.update_input_region();
        } else {
            let w = self.default_width();
            let h = self.default_height();
            if w > 0 { imp.compact_width.set(w); }
            if h > 0 { imp.compact_height.set(h); }
            imp.is_expanded.set(true);
            if let Some(overlay) = imp.overlay_container.borrow().as_ref() {
                overlay.remove_css_class("circle");
                overlay.remove_css_class("rounded-rect");
            }
            self.fullscreen();
        }
    }

    fn setup_context_menu(&self) {
        let menu = gio::Menu::new();

        let zoom_section = gio::Menu::new();
        zoom_section.append(Some("1×"), Some("win.zoom::1"));
        zoom_section.append(Some("1.5×"), Some("win.zoom::2"));
        zoom_section.append(Some("2×"), Some("win.zoom::3"));
        menu.append_section(Some("Zoom"), &zoom_section);

        let shape_section = gio::Menu::new();
        shape_section.append(Some("Circle"), Some("win.shape::circle"));
        shape_section.append(Some("Rounded Rectangle"), Some("win.shape::rounded-rect"));
        menu.append_section(Some("Shape"), &shape_section);

        let fit_section = gio::Menu::new();
        fit_section.append(Some("Crop"), Some("win.fit::cover"));
        fit_section.append(Some("Stretch"), Some("win.fit::fill"));
        menu.append_section(Some("Scale"), &fit_section);

        let mirror_section = gio::Menu::new();
        mirror_section.append(Some("Mirror"), Some("win.flip"));
        menu.append_section(None, &mirror_section);

        let about_section = gio::Menu::new();
        about_section.append(Some("About"), Some("app.about"));
        menu.append_section(None, &about_section);

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        popover.set_has_arrow(false);
        popover.set_parent(self);

        let right_click = gtk4::GestureClick::new();
        right_click.set_button(3);

        right_click.connect_pressed(move |gesture, _, x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let rect = gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            popover.set_pointing_to(Some(&rect));
            popover.popup();
        });

        self.add_controller(right_click);
    }

    fn setup_actions(&self) {
        let settings = self.imp().settings.borrow().clone().expect("Settings not initialized");

        // Zoom action
        let zoom_level = settings.int("zoom-level").to_string();
        let zoom_action = gio::SimpleAction::new_stateful(
            "zoom",
            Some(&glib::VariantTy::STRING),
            &zoom_level.to_variant(),
        );
        let win = self.clone();
        zoom_action.connect_activate(move |action, param| {
            if let Some(v) = param {
                let level_str: String = v.get().unwrap_or_default();
                let level: i32 = level_str.parse().unwrap_or(1);
                action.set_state(&level_str.to_variant());
                win.apply_zoom(level);
                if let Some(s) = win.imp().settings.borrow().as_ref() {
                    let _ = s.set_int("zoom-level", level);
                }
            }
        });
        self.add_action(&zoom_action);

        // Shape action
        let current_shape = settings.string("shape").to_string();
        let shape_action = gio::SimpleAction::new_stateful(
            "shape",
            Some(&glib::VariantTy::STRING),
            &current_shape.to_variant(),
        );
        let win = self.clone();
        shape_action.connect_activate(move |action, param| {
            if let Some(v) = param {
                let shape: String = v.get().unwrap_or_default();
                action.set_state(&shape.to_variant());
                win.apply_shape(&shape);
                if let Some(s) = win.imp().settings.borrow().as_ref() {
                    let _ = s.set_string("shape", &shape);
                }
            }
        });
        self.add_action(&shape_action);

        // Fit mode action
        let current_fit = settings.string("fit-mode").to_string();
        let fit_action = gio::SimpleAction::new_stateful(
            "fit",
            Some(&glib::VariantTy::STRING),
            &current_fit.to_variant(),
        );
        let win = self.clone();
        fit_action.connect_activate(move |action, param| {
            if let Some(v) = param {
                let mode: String = v.get().unwrap_or_default();
                action.set_state(&mode.to_variant());
                win.apply_fit_mode(&mode);
                if let Some(s) = win.imp().settings.borrow().as_ref() {
                    let _ = s.set_string("fit-mode", &mode);
                }
            }
        });
        self.add_action(&fit_action);

        // Flip action
        let flipped = settings.boolean("flipped");
        let flip_action = gio::SimpleAction::new_stateful("flip", None, &flipped.to_variant());
        let win = self.clone();
        flip_action.connect_activate(move |action, _| {
            let current: bool = action.state().and_then(|v| v.get()).unwrap_or(false);
            let new_state = !current;
            action.set_state(&new_state.to_variant());
            win.apply_flip(new_state);
            if let Some(s) = win.imp().settings.borrow().as_ref() {
                let _ = s.set_boolean("flipped", new_state);
            }
        });
        self.add_action(&flip_action);
    }

    fn apply_zoom(&self, level: i32) {
        let imp = self.imp();
        let width = imp.video_width.get();
        let height = imp.video_height.get();
        if width == 0 || height == 0 {
            return;
        }
        let (left, right, top, bottom) = match level {
            2 => { let lr = width / 6; let tb = height / 6; (lr, lr, tb, tb) }
            3 => { let lr = width / 4; let tb = height / 4; (lr, lr, tb, tb) }
            _ => (0, 0, 0, 0),
        };
        if let Some(pipeline) = imp.pipeline.borrow().as_ref() {
            if let Some(bin) = pipeline.downcast_ref::<gstreamer::Bin>() {
                if let Some(cropper) = bin.by_name("cropper") {
                    cropper.set_property("left", left);
                    cropper.set_property("right", right);
                    cropper.set_property("top", top);
                    cropper.set_property("bottom", bottom);
                }
            }
        }
    }

    fn update_input_region(&self) {
        use gtk4::prelude::NativeExt;
        let Some(surface) = self.upcast_ref::<gtk4::Window>().surface() else { return; };
        let imp = self.imp();

        let is_shaped = !imp.is_expanded.get()
            && imp.overlay_container
                .borrow()
                .as_ref()
                .map(|o| o.has_css_class("circle") || o.has_css_class("rounded-rect"))
                .unwrap_or(false);

        // Always use full window for input so resize handles work at all edges
        let full = gtk4::cairo::Region::create_rectangle(
            &gtk4::cairo::RectangleInt::new(0, 0, self.width(), self.height())
        );
        surface.set_input_region(&full);

        // For shaped windows, don't hint the compositor with an approximated opaque region —
        // a pixel-row circle approximation is jagged and causes compositor artifacts at the
        // edges. Setting None lets the compositor composite correctly from the alpha channel.
        // For expanded (rectangular) windows, the full region is correct.
        if is_shaped {
            surface.set_opaque_region(None);
        } else {
            surface.set_opaque_region(Some(&full));
        }
    }

    fn apply_shape(&self, shape: &str) {
        let imp = self.imp();
        if imp.is_expanded.get() {
            return;
        }
        if let Some(overlay) = imp.overlay_container.borrow().as_ref() {
            overlay.remove_css_class("circle");
            overlay.remove_css_class("rounded-rect");
            overlay.add_css_class(shape);
        }
        if shape == "circle" {
            let w = imp.compact_width.get().max(1);
            let h = imp.compact_height.get().max(1);
            let size = w.min(h);
            self.set_default_size(size, size);
            imp.compact_width.set(size);
            imp.compact_height.set(size);
        }
        self.update_input_region();
    }

    fn apply_fit_mode(&self, mode: &str) {
        let fit = match mode {
            "fill" => gtk4::ContentFit::Fill,
            _ => gtk4::ContentFit::Cover,
        };
        if let Some(picture) = self.imp().video_picture.borrow().as_ref() {
            picture.set_content_fit(fit);
        }
    }

    fn apply_flip(&self, flipped: bool) {
        if let Some(pipeline) = self.imp().pipeline.borrow().as_ref() {
            if let Some(bin) = pipeline.downcast_ref::<gstreamer::Bin>() {
                if let Some(flipper) = bin.by_name("flipper") {
                    flipper.set_property_from_str("method", if flipped { "horizontal-flip" } else { "none" });
                }
            }
        }
    }
}
