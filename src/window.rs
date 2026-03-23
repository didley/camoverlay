use adw::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::prelude::NativeExt;
use glib::prelude::IsA;
use gtk4::{gdk, gio, glib};
use gstreamer::prelude::*;
use gstreamer::prelude::DeviceExt as GstDeviceExt;
use gstreamer::MessageView;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const SETTINGS_SCHEMA: &str = "io.github.didley.CamOverlay";
const RESIZE_BORDER: f64 = 16.0;
const SHAPE_BORDER_WIDTH: f32 = 2.0;
const CIRCLE_GRAB_RADIUS_OFFSET: f64 = 8.0;
const ROUNDED_RECT_RADIUS: f32 = 16.0;
const ZOOM_L2_CROP_FRACTION: i32 = 6; // 1/6th per side → 1.5× zoom
const ZOOM_L3_CROP_FRACTION: i32 = 4; // 1/4th per side → 2× zoom

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Circle,
    RoundedRect,
}

impl Shape {
    fn from_str(s: &str) -> Self {
        match s {
            "circle" => Self::Circle,
            _ => Self::RoundedRect,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::RoundedRect => "rounded-rect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    Cover,
    Fill,
}

impl FitMode {
    fn from_str(s: &str) -> Self {
        match s {
            "fill" => Self::Fill,
            _ => Self::Cover,
        }
    }

    fn to_gtk(self) -> gtk4::ContentFit {
        match self {
            Self::Fill => gtk4::ContentFit::Fill,
            Self::Cover => gtk4::ContentFit::Cover,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ZoomLevel {
    One,
    OnePointFive,
    Two,
}

impl ZoomLevel {
    fn from_i32(n: i32) -> Self {
        match n {
            2 => Self::OnePointFive,
            3 => Self::Two,
            _ => Self::One,
        }
    }

    fn crop_fraction(self) -> Option<i32> {
        match self {
            Self::One => None,
            Self::OnePointFive => Some(ZOOM_L2_CROP_FRACTION),
            Self::Two => Some(ZOOM_L3_CROP_FRACTION),
        }
    }
}

/// Returns (cx, cy, r) for the inscribed circle of a w×h rectangle.
fn circle_geometry(w: f64, h: f64) -> (f64, f64, f64) {
    let r = w.min(h) / 2.0;
    (w / 2.0, h / 2.0, r)
}

fn device_id(device: &gstreamer::Device) -> Option<String> {
    let props = device.properties()?;
    props.get::<String>("object.serial").ok()
        .or_else(|| props.get::<i32>("object.serial").ok().map(|n: i32| n.to_string()))
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct CamOverlayWindow {
        pub pipeline: RefCell<Option<gstreamer::Element>>,
        pub pipeline_generation: Cell<u32>,
        pub settings: RefCell<Option<gio::Settings>>,
        pub overlay_container: RefCell<Option<gtk4::Overlay>>,
        pub video_picture: RefCell<Option<gtk4::Picture>>,
        pub is_expanded: Cell<bool>,
        pub compact_width: Cell<i32>,
        pub compact_height: Cell<i32>,
        pub video_width: Cell<i32>,
        pub video_height: Cell<i32>,
        pub cursor_edge: RefCell<Option<gdk::SurfaceEdge>>,
        pub device_monitor: RefCell<Option<gstreamer::DeviceMonitor>>,
        pub camera_menu: RefCell<Option<gio::Menu>>,
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
            if let Some(monitor) = self.device_monitor.borrow().as_ref() {
                monitor.stop();
            }
            if let Some(pipeline) = self.pipeline.borrow().as_ref() {
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
        }
    }

    impl WidgetImpl for CamOverlayWindow {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let w = widget.width() as f32;
            let h = widget.height() as f32;
            let border_color = [gdk::RGBA::new(0.0, 0.0, 0.0, 0.4); 4];
            let border_width = [SHAPE_BORDER_WIDTH; 4];

            match widget.current_shape() {
                Some(Shape::Circle) => {
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
                }
                Some(Shape::RoundedRect) => {
                    let rounded = gtk4::gsk::RoundedRect::from_rect(
                        gtk4::graphene::Rect::new(0.0, 0.0, w, h),
                        ROUNDED_RECT_RADIUS,
                    );
                    snapshot.push_rounded_clip(&rounded);
                    self.parent_snapshot(snapshot);
                    snapshot.append_border(&rounded, &border_width, &border_color);
                    snapshot.pop();
                }
                None => {
                    self.parent_snapshot(snapshot);
                }
            }
        }

        fn contains(&self, x: f64, y: f64) -> bool {
            let widget = self.obj();
            if widget.current_shape() == Some(Shape::Circle) {
                let (cx, cy, r) = circle_geometry(widget.width() as f64, widget.height() as f64);
                let dx = x - cx;
                let dy = y - cy;
                // Circle interior + thin ring outside the edge for resize handle grab
                let grab_r = r + CIRCLE_GRAB_RADIUS_OFFSET;
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

    fn settings(&self) -> std::cell::Ref<'_, gio::Settings> {
        std::cell::Ref::map(self.imp().settings.borrow(), |s| {
            s.as_ref().expect("settings accessed before init_window")
        })
    }

    fn pipeline_element(&self, name: &str) -> Option<gstreamer::Element> {
        let pipeline = self.imp().pipeline.borrow();
        let bin = pipeline.as_ref()?.downcast_ref::<gstreamer::Bin>()?;
        bin.by_name(name)
    }

    fn current_shape(&self) -> Option<Shape> {
        if self.imp().is_expanded.get() {
            return None;
        }
        let overlay = self.imp().overlay_container.borrow();
        let o = overlay.as_ref()?;
        if o.has_css_class("circle") {
            Some(Shape::Circle)
        } else if o.has_css_class("rounded-rect") {
            Some(Shape::RoundedRect)
        } else {
            None
        }
    }

    fn init_window(&self) {
        let imp = self.imp();

        let settings = gio::Settings::new(SETTINGS_SCHEMA);
        *imp.settings.borrow_mut() = Some(settings.clone());

        self.add_css_class("camoverlay");
        self.set_decorated(false);
        self.set_resizable(true);
        self.set_size_request(60, 60);

        let saved_width = settings.int("window-width");
        let saved_height = settings.int("window-height");
        let shape = Shape::from_str(&settings.string("shape"));
        let (init_w, init_h) = if shape == Shape::Circle {
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

        video_picture.set_content_fit(FitMode::from_str(&settings.string("fit-mode")).to_gtk());

        overlay_container.set_child(Some(&video_picture));
        self.set_child(Some(&overlay_container));

        *imp.overlay_container.borrow_mut() = Some(overlay_container.clone());
        *imp.video_picture.borrow_mut() = Some(video_picture);

        overlay_container.add_css_class(shape.as_str());

        self.setup_device_monitor();
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
                    if win.settings().string("shape") == "circle" {
                        let size = w.min(h);
                        w = size;
                        h = size;
                        win.set_default_size(size, size);
                    }
                    win.imp().compact_width.set(w);
                    win.imp().compact_height.set(h);
                    let _ = win.settings().set_int("window-width", w);
                    let _ = win.settings().set_int("window-height", h);
                }
            }
            win.update_input_region();
        });
    }

    fn setup_pipeline(&self) {
        let imp = self.imp();

        // Stop existing pipeline
        if let Some(pipeline) = imp.pipeline.borrow().as_ref() {
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        *imp.pipeline.borrow_mut() = None;
        imp.video_width.set(0);
        imp.video_height.set(0);

        // Determine which camera to use
        let mut camera_serial = self.settings().string("camera-id").to_string();

        // If no saved camera or saved camera not found, pick the first available
        if camera_serial.is_empty() || !self.camera_exists(&camera_serial) {
            if let Some(monitor) = imp.device_monitor.borrow().as_ref() {
                let devices = monitor.devices();
                if let Some(first) = devices.iter().next() {
                    if let Some(serial) = device_id(&first) {
                        camera_serial = serial.clone();
                        let _ = self.settings().set_string("camera-id", &serial);
                    }
                }
            }
        }

        // Create source element — use device.create_element() for proper
        // PipeWire node configuration (just setting pipewiresrc path= is
        // not enough to activate some cameras).
        let src = if !camera_serial.is_empty() {
            self.find_device(&camera_serial)
                .and_then(|dev| dev.create_element(Some("src")).ok())
        } else {
            None
        };
        let src = src.unwrap_or_else(|| {
            gstreamer::ElementFactory::make("pipewiresrc")
                .name("src")
                .build()
                .expect("Failed to create pipewiresrc")
        });

        let converter = gstreamer::ElementFactory::make("videoconvert")
            .name("converter")
            .build()
            .expect("Failed to create videoconvert");
        let flipper = gstreamer::ElementFactory::make("videoflip")
            .name("flipper")
            .build()
            .expect("Failed to create videoflip");
        flipper.set_property_from_str("method", "none");
        let cropper = gstreamer::ElementFactory::make("videocrop")
            .name("cropper")
            .build()
            .expect("Failed to create videocrop");
        let scaler = gstreamer::ElementFactory::make("videoscale")
            .build()
            .expect("Failed to create videoscale");
        let sink = gstreamer::ElementFactory::make("gtk4paintablesink")
            .name("sink")
            .build()
            .expect("Failed to create gtk4paintablesink");
        sink.set_property("sync", false);

        let pipeline = gstreamer::Pipeline::new();
        pipeline.add(&src).expect("Failed to add src");
        pipeline.add(&converter).expect("Failed to add converter");
        pipeline.add(&flipper).expect("Failed to add flipper");
        pipeline.add(&cropper).expect("Failed to add cropper");
        pipeline.add(&scaler).expect("Failed to add scaler");
        pipeline.add(&sink).expect("Failed to add sink");

        src.link(&converter).expect("Failed to link src→converter");
        converter.link(&flipper).expect("Failed to link converter→flipper");
        flipper.link(&cropper).expect("Failed to link flipper→cropper");
        cropper.link(&scaler).expect("Failed to link cropper→scaler");
        scaler.link(&sink).expect("Failed to link scaler→sink");

        let paintable = sink.property::<gdk::Paintable>("paintable");
        if let Some(picture) = imp.video_picture.borrow().as_ref() {
            picture.set_paintable(Some(&paintable));
        }

        // Apply saved flip
        if self.settings().boolean("flipped") {
            flipper.set_property_from_str("method", "horizontal-flip");
        }

        let pipeline_element = pipeline.upcast::<gstreamer::Element>();
        if let Err(e) = pipeline_element.set_state(gstreamer::State::Playing) {
            eprintln!("Failed to start pipeline: {e}");
        }

        *imp.pipeline.borrow_mut() = Some(pipeline_element);

        // Bump generation so any old caps-polling closure exits
        let generation = imp.pipeline_generation.get() + 1;
        imp.pipeline_generation.set(generation);

        // Poll for negotiated caps on the main thread, then apply saved zoom + fit
        let win = self.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let imp = win.imp();
            if imp.pipeline_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
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
                let zoom = ZoomLevel::from_i32(win.settings().int("zoom-level"));
                let fit_mode = FitMode::from_str(&win.settings().string("fit-mode"));
                drop(pipeline_ref);
                win.apply_zoom(zoom);
                win.apply_fit_mode(fit_mode);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn find_device(&self, serial: &str) -> Option<gstreamer::Device> {
        let monitor = self.imp().device_monitor.borrow();
        let monitor = monitor.as_ref()?;
        monitor.devices().iter()
            .find(|d| device_id(d).as_deref() == Some(serial))
            .map(|d| (*d).clone())
    }

    fn camera_exists(&self, serial: &str) -> bool {
        self.imp().device_monitor.borrow().as_ref()
            .map(|m| m.devices().iter().any(|d| device_id(&d).as_deref() == Some(serial)))
            .unwrap_or(false)
    }

    fn setup_drag(&self) {
        let drag = gtk4::GestureDrag::new();
        drag.set_button(1);

        let initiated = Rc::new(Cell::new(false));
        let edge_at_press: Rc<Cell<Option<gdk::SurfaceEdge>>> = Rc::new(Cell::new(None));

        let win = self.clone();
        let initiated_begin = initiated.clone();
        let edge_begin = edge_at_press.clone();
        drag.connect_drag_begin(move |_, _, _| {
            initiated_begin.set(false);
            edge_begin.set(*win.imp().cursor_edge.borrow());
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
        let motion = gtk4::EventControllerMotion::new();
        let win = self.clone();
        motion.connect_motion(move |_, x, y| {
            let imp = win.imp();
            // Use actual allocated size — default_width() lags during a WM-driven resize
            let w = win.width() as f64;
            let h = win.height() as f64;

            let edge = if win.current_shape() == Some(Shape::Circle) {
                let (cx, cy, r) = circle_geometry(w, h);
                let dx = x - cx;
                let dy = y - cy;
                let dist_sq = dx * dx + dy * dy;
                let inner = r - RESIZE_BORDER;
                // Near the circle edge → corner resize based on quadrant so both
                // dimensions change together and the window stays square
                if dist_sq >= inner * inner {
                    Some(match (x < cx, y < cy) {
                        (true,  true)  => gdk::SurfaceEdge::NorthWest,
                        (true,  false) => gdk::SurfaceEdge::SouthWest,
                        (false, true)  => gdk::SurfaceEdge::NorthEast,
                        (false, false) => gdk::SurfaceEdge::SouthEast,
                    })
                } else {
                    None // inside circle → move gesture
                }
            } else {
                let left   = x < RESIZE_BORDER;
                let right  = x > w - RESIZE_BORDER;
                let top    = y < RESIZE_BORDER;
                let bottom = y > h - RESIZE_BORDER;
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
                let shape = self.settings().string("shape").to_string();
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
                overlay.remove_css_class(Shape::Circle.as_str());
                overlay.remove_css_class(Shape::RoundedRect.as_str());
            }
            self.fullscreen();
            self.update_input_region();
        }
    }

    fn setup_context_menu(&self) {
        let menu = gio::Menu::new();

        // Camera section (dynamically populated)
        let camera_menu = self.imp().camera_menu.borrow().clone()
            .unwrap_or_else(gio::Menu::new);
        menu.append_section(Some("Camera"), &camera_menu);

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
        about_section.append(Some("Quit"), Some("app.quit"));
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

        // Camera action
        let current_camera = settings.string("camera-id").to_string();
        let camera_action = gio::SimpleAction::new_stateful(
            "camera",
            Some(&glib::VariantTy::STRING),
            &current_camera.to_variant(),
        );
        let win = self.clone();
        camera_action.connect_activate(move |action, param| {
            if let Some(v) = param {
                let serial: String = v.get().unwrap_or_default();
                action.set_state(&serial.to_variant());
                let _ = win.settings().set_string("camera-id", &serial);
                win.setup_pipeline();
            }
        });
        self.add_action(&camera_action);

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
                let level_i32: i32 = level_str.parse().unwrap_or(1);
                action.set_state(&level_str.to_variant());
                win.apply_zoom(ZoomLevel::from_i32(level_i32));
                let _ = win.settings().set_int("zoom-level", level_i32);
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
                let shape_str: String = v.get().unwrap_or_default();
                action.set_state(&shape_str.to_variant());
                win.apply_shape(Shape::from_str(&shape_str));
                let _ = win.settings().set_string("shape", &shape_str);
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
                let mode_str: String = v.get().unwrap_or_default();
                action.set_state(&mode_str.to_variant());
                win.apply_fit_mode(FitMode::from_str(&mode_str));
                let _ = win.settings().set_string("fit-mode", &mode_str);
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
            let _ = win.settings().set_boolean("flipped", new_state);
        });
        self.add_action(&flip_action);
    }

    fn setup_device_monitor(&self) {
        let monitor = gstreamer::DeviceMonitor::new();
        monitor.add_filter(Some("Video/Source"), None);

        if monitor.start().is_err() {
            eprintln!("Failed to start device monitor");
            return;
        }

        // Create the camera menu and populate it
        let camera_menu = gio::Menu::new();
        *self.imp().camera_menu.borrow_mut() = Some(camera_menu.clone());
        self.rebuild_camera_menu(&monitor.devices());

        // Watch for device additions/removals
        let bus = monitor.bus();
        let win_weak = self.downgrade();
        bus.add_watch_local(move |_, msg| {
            let Some(win) = win_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            match msg.view() {
                MessageView::DeviceAdded(_) | MessageView::DeviceRemoved(_) => {
                    if let Some(monitor) = win.imp().device_monitor.borrow().as_ref() {
                        win.rebuild_camera_menu(&monitor.devices());
                    }
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        }).ok();

        *self.imp().device_monitor.borrow_mut() = Some(monitor);
    }

    fn rebuild_camera_menu(&self, devices: &glib::List<gstreamer::Device>) {
        let imp = self.imp();
        let camera_menu_ref = imp.camera_menu.borrow();
        let Some(menu) = camera_menu_ref.as_ref() else { return };

        // Clear existing items
        while menu.n_items() > 0 {
            menu.remove(0);
        }

        for device in devices.iter() {
            let name = GstDeviceExt::display_name(&*device);
            if let Some(serial) = device_id(&device) {
                let action_target = format!("win.camera::{serial}");
                menu.append(Some(&name), Some(&action_target));
            }
        }
    }

    fn apply_zoom(&self, level: ZoomLevel) {
        let imp = self.imp();
        let width = imp.video_width.get();
        let height = imp.video_height.get();
        if width == 0 || height == 0 {
            return;
        }
        let (left, right, top, bottom) = match level.crop_fraction() {
            Some(f) => { let lr = width / f; let tb = height / f; (lr, lr, tb, tb) }
            None => (0, 0, 0, 0),
        };
        if let Some(cropper) = self.pipeline_element("cropper") {
            cropper.set_property("left", left);
            cropper.set_property("right", right);
            cropper.set_property("top", top);
            cropper.set_property("bottom", bottom);
        }
    }

    fn update_input_region(&self) {
        let Some(surface) = self.upcast_ref::<gtk4::Window>().surface() else { return; };
        let imp = self.imp();

        if imp.is_expanded.get() {
            // Fullscreen: accept input everywhere.  self.width()/height()
            // may still reflect the compact size right after fullscreen()
            // because Wayland configure events are async, so use a value
            // large enough for any display — the compositor clamps to the
            // actual surface bounds.
            let full = gtk4::cairo::Region::create_rectangle(
                &gtk4::cairo::RectangleInt::new(0, 0, 32767, 32767),
            );
            surface.set_input_region(&full);
            surface.set_opaque_region(None);
            return;
        }

        let is_shaped = self.current_shape().is_some();

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

    fn apply_shape(&self, shape: Shape) {
        let imp = self.imp();
        if imp.is_expanded.get() {
            return;
        }
        if let Some(overlay) = imp.overlay_container.borrow().as_ref() {
            overlay.remove_css_class(Shape::Circle.as_str());
            overlay.remove_css_class(Shape::RoundedRect.as_str());
            overlay.add_css_class(shape.as_str());
        }
        if shape == Shape::Circle {
            let w = imp.compact_width.get().max(1);
            let h = imp.compact_height.get().max(1);
            let size = w.min(h);
            self.set_default_size(size, size);
            imp.compact_width.set(size);
            imp.compact_height.set(size);
        }
        self.update_input_region();
    }

    fn apply_fit_mode(&self, mode: FitMode) {
        if let Some(picture) = self.imp().video_picture.borrow().as_ref() {
            picture.set_content_fit(mode.to_gtk());
        }
    }

    fn apply_flip(&self, flipped: bool) {
        if let Some(flipper) = self.pipeline_element("flipper") {
            flipper.set_property_from_str("method", if flipped { "horizontal-flip" } else { "none" });
        }
    }
}
