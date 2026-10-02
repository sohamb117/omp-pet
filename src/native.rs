//! AppKit objects and callbacks stay on the main thread.
use crate::{
    geometry::{PET_SIZE, Placement, Point, Rect},
    hover::HoverReveal,
    ipc::{Control, Event},
    model::{Activity, ContextUsage, Snapshot},
    preferences,
    sessions::Sessions,
    sprites::SpritePack,
};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSTimer,
};
use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
};

thread_local! { static UI: RefCell<Option<AppUi>> = const { RefCell::new(None) }; }
fn with_ui(f: impl FnOnce(&mut AppUi)) {
    UI.with(|slot| {
        if let Some(ui) = slot.borrow_mut().as_mut() {
            f(ui);
        }
    });
}

#[derive(Default)]
struct PetIvars {
    phase: Cell<usize>,
    activity: Cell<Activity>,
    context: RefCell<Option<ContextUsage>>,
    tucked: Cell<bool>,
    hovered: Cell<bool>,
    celebrate: Cell<bool>,
    sprites: RefCell<Option<SpritePack>>,
}

define_class!(
    // SAFETY: NSView subclass, used exclusively on the main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PetIvars]
    struct PetView;
    unsafe impl NSObjectProtocol for PetView {}
    impl PetView {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            if self.ivars().tucked.get() {
                let rgb=self.ivars().sprites.borrow().as_ref().map_or(crate::palette::FALLBACK,|p|p.accent);
                rounded(self.bounds(), 1.5, &color(rgb[0] as f64/255.,rgb[1] as f64/255.,rgb[2] as f64/255.,0.9));
            } else {
                NSGraphicsContext::saveGraphicsState_class();
                let transform=objc2_foundation::NSAffineTransform::transform();
                transform.scaleBy(self.bounds().size.width/PET_SIZE); transform.concat();
                draw_pet(self.ivars());
                NSGraphicsContext::restoreGraphicsState_class();
                if self.ivars().hovered.get() {
                    oval(rect(self.bounds().size.width-6.,3.,3.,3.), &color(0.75,0.75,0.75,0.35));
                }
            }
        }
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) { with_ui(|ui| {
            if ui.resize_hit(cursor()) || event.modifierFlags().contains(NSEventModifierFlags::Option) { ui.begin_resize(); } else { ui.begin_drag(); }
        }); }
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) { with_ui(|ui| ui.drag()); }
        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) { with_ui(|ui| ui.end_drag()); }
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Do not retain the UI borrow while AppKit runs the menu's nested loop.
            let menu = UI.with(|slot| slot.borrow().as_ref().map(|ui| ui.menu.clone()));
            if let Some(menu) = menu { NSMenu::popUpContextMenu_withEvent_forView(&menu, event, self); }
        }
    }
);
impl PetView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PetIvars::default());
        unsafe {
            msg_send![super(this), initWithFrame: nsrect(Rect { x: 0., y: 0., w: PET_SIZE, h: PET_SIZE })]
        }
    }
}

define_class!(
    // SAFETY: NSPanel subclass. Pet and hover card cannot become keyboard targets.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    struct PetPanel;
    unsafe impl NSObjectProtocol for PetPanel {}
    impl PetPanel {
        #[unsafe(method(accessibilityPerformRaise))]
        fn accessibility_raise(&self) -> bool { self.orderFrontRegardless(); true }
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

struct CardIvars {
    accent: Cell<[u8; 3]>,
}
impl Default for CardIvars {
    fn default() -> Self {
        Self {
            accent: Cell::new(crate::palette::FALLBACK),
        }
    }
}
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = CardIvars]
    struct CardView;
    unsafe impl NSObjectProtocol for CardView {}
    impl CardView {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            let c=self.ivars().accent.get();
            rounded(self.bounds(),8.,&color(c[0] as f64/255.*0.45,c[1] as f64/255.*0.45,c[2] as f64/255.*0.45,0.70));
        }
    }
);

fn cursor() -> Point {
    let p = NSEvent::mouseLocation();
    Point { x: p.x, y: p.y }
}
fn cancel(timer: &mut Option<Retained<NSTimer>>) {
    if let Some(t) = timer.take() {
        t.invalidate();
    }
}
fn screen_at(p: Point, mtm: MainThreadMarker) -> Rect {
    let screens = NSScreen::screens(mtm);
    screens
        .iter()
        .find(|s| rustrect(s.frame()).contains(p))
        .or_else(|| screens.iter().next())
        .map(|s| rustrect(s.visibleFrame()))
        .unwrap_or(Rect {
            x: 0.,
            y: 0.,
            w: 1280.,
            h: 720.,
        })
}
fn nsrect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    nsrect(Rect { x, y, w, h })
}
fn rustrect(r: NSRect) -> Rect {
    Rect {
        x: r.origin.x,
        y: r.origin.y,
        w: r.size.width,
        h: r.size.height,
    }
}
fn color(r: f64, g: f64, b: f64, a: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a)
}
fn rounded(bounds: NSRect, radius: f64, fill: &NSColor) {
    fill.setFill();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius).fill();
}
fn oval(bounds: NSRect, fill: &NSColor) {
    fill.setFill();
    NSBezierPath::bezierPathWithOvalInRect(bounds).fill();
}
fn draw_pet(ivars: &PetIvars) {
    let ink = color(0.10, 0.13, 0.18, 1.);
    let mint = color(0.64, 0.89, 0.77, 1.);
    if let Some(pack) = ivars.sprites.borrow().as_ref() {
        pack.draw(
            ivars.activity.get(),
            ivars.phase.get(),
            ivars.celebrate.get(),
            rect(0., 0., PET_SIZE, PET_SIZE),
        );
    } else {
        NSGraphicsContext::saveGraphicsState_class();
        let transform = objc2_foundation::NSAffineTransform::transform();
        transform.translateXBy_yBy(56., 56.);
        transform.scaleBy(112. / 68.);
        transform.translateXBy_yBy(-57., -61.);
        transform.concat();
        oval(rect(23., 19., 67., 9.), &color(0., 0., 0., 0.12));
        rounded(rect(26., 27., 62., 58.), 23., &ink);
        oval(rect(26., 70., 23., 25.), &ink);
        oval(rect(65., 70., 23., 25.), &ink);
        rounded(rect(30., 31., 54., 50.), 20., &mint);
        oval(rect(30., 74., 15., 17.), &mint);
        oval(rect(69., 74., 15., 17.), &mint);
        oval(rect(41., 54., 6., 9.), &ink);
        oval(rect(67., 54., 6., 9.), &ink);
        rounded(rect(52., 46., 10., 3.), 1.5, &ink);
        oval(rect(34., 47., 9., 5.), &color(0.96, 0.62, 0.64, 0.75));
        oval(rect(73., 47., 9., 5.), &color(0.96, 0.62, 0.64, 0.75));
        NSGraphicsContext::restoreGraphicsState_class();
    }
    rounded(rect(29., 4., 54., 4.), 2., &color(0.10, 0.13, 0.18, 0.35));
    if let Some(c) = ivars.context.borrow().as_ref() {
        let rgb = ivars
            .sprites
            .borrow()
            .as_ref()
            .map_or(crate::palette::FALLBACK, |pack| pack.accent);
        let fill = color(
            rgb[0] as f64 / 255.,
            rgb[1] as f64 / 255.,
            rgb[2] as f64 / 255.,
            1.,
        );
        rounded(rect(29., 4., 54. * c.percent / 100., 4.), 2., &fill);
    }
    let activity = ivars.activity.get();
    if activity != Activity::Idle {
        rounded(rect(77., 80., 32., 20.), 7., &ink);
        rounded(rect(80., 77., 7., 7.), 2., &ink);
        let tint = match activity {
            Activity::Error => color(1., 0.45, 0.45, 1.),
            Activity::Waiting => color(1., 0.76, 0.39, 1.),
            Activity::Disconnected => color(0.55, 0.60, 0.66, 1.),
            _ => mint,
        };
        for i in 0..3 {
            let alpha = if activity.animates() && i != ivars.phase.get() % 3 {
                0.3
            } else {
                1.
            };
            oval(
                rect(83. + i as f64 * 8., 87., 4., 4.),
                &tint.colorWithAlphaComponent(alpha),
            );
        }
    }
}
fn panel(mtm: MainThreadMarker, bounds: NSRect, title: &str) -> Retained<PetPanel> {
    let panel: Retained<PetPanel> = unsafe {
        msg_send![PetPanel::alloc(mtm),
        initWithContentRect:bounds, styleMask:NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        backing:NSBackingStoreType::Buffered, defer:false]
    };
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setTitle(&NSString::from_str(title));
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setLevel(3);
    panel.setHidesOnDeactivate(false);
    panel.setMovable(false); // All dragging is handled by PetView, never the window server.
    panel.setMovableByWindowBackground(false);
    panel.setExcludedFromWindowsMenu(true);
    panel.setContentMinSize(bounds.size);
    panel.setContentMaxSize(bounds.size);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenDisallowsTiling,
    );
    panel
}

fn timer(seconds: f64, repeats: bool, f: impl Fn() + 'static) -> Retained<NSTimer> {
    let block = RcBlock::new(move |_timer: NonNull<NSTimer>| f());
    // SAFETY: the block runs on the main run loop and captures no thread-bound pointers.
    let timer =
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(seconds, repeats, &block) };
    timer.setTolerance(if repeats { seconds * 0.1 } else { 0.02 });
    timer
}

struct AppUi {
    pet: Retained<PetPanel>,
    view: Retained<PetView>,
    card: Retained<PetPanel>,
    labels: Vec<Retained<NSTextField>>,
    card_view: Retained<CardView>,
    menu: Retained<NSMenu>,
    _status: Retained<NSStatusItem>,
    sessions: Sessions,
    placement: Placement,
    animation: Option<Retained<NSTimer>>,
    readout_pinned: bool,
    reveal_timer: Option<Retained<NSTimer>>,
    drag_offset: Option<Point>,
    resize_start: Option<(Point, Placement)>,
    reveal_armed: bool,
    hover_reveal: HoverReveal,
    monitors: Vec<Retained<AnyObject>>,
    edge_watch: Option<Retained<NSTimer>>,
}
impl AppUi {
    fn new(mtm: MainThreadMarker, delegate: &Delegate, demo: bool, readout: bool) -> Self {
        let screen = rustrect(
            NSScreen::mainScreen(mtm)
                .expect("a screen is required")
                .visibleFrame(),
        );
        let prefs = preferences::load();
        let mut placement = prefs.placement.unwrap_or_else(|| Placement::new(screen));
        placement.screen = screen_at(
            Point {
                x: placement.origin.x + PET_SIZE / 2.,
                y: placement.origin.y + PET_SIZE / 2.,
            },
            mtm,
        );
        placement.size = placement.size.clamp(64., 256.);
        placement.align();
        let pet = panel(mtm, nsrect(placement.frame(false)), "OMP Pet");
        let view = PetView::new(mtm);
        pet.setContentView(Some(&view));
        view.setAccessibilityElement(true);
        view.setAccessibilityRole(Some(unsafe { NSAccessibilityImageRole }));
        view.setAccessibilityLabel(Some(&NSString::from_str("OMP Pet desktop companion")));
        pet.setAcceptsMouseMovedEvents(true);
        if let Some(path) = std::env::var_os("OMP_PET_SPRITES")
            .map(std::path::PathBuf::from)
            .or(prefs.sprites)
        {
            match SpritePack::load(&path, mtm) {
                Ok(pack) => *view.ivars().sprites.borrow_mut() = Some(pack),
                Err(e) => eprintln!("Could not load sprite pack: {e}"),
            }
        }
        let card = panel(mtm, rect(0., 0., 200., 76.), "OMP Pet — task readout");
        let card_alloc = CardView::alloc(mtm).set_ivars(CardIvars::default());
        let content: Retained<CardView> =
            unsafe { msg_send![super(card_alloc),initWithFrame:rect(0.,0.,200.,76.)] };
        let mut labels = Vec::new();
        for (y, height, size) in [(55., 14., 10.), (28., 22., 11.), (6., 14., 10.)] {
            let label = NSTextField::labelWithString(&NSString::from_str(""), mtm);
            label.setFrame(rect(8., y, 184., height));
            label.setFont(Some(&NSFont::systemFontOfSize(size)));
            label.setTextColor(Some(&color(0.90, 0.94, 0.96, 1.)));
            label.setMaximumNumberOfLines(if height > 20. { 2 } else { 1 });
            content.addSubview(&label);
            labels.push(label);
        }
        card.setContentView(Some(&content));
        card.setHasShadow(true);
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        for (title, action) in [
            ("Pin readout", sel!(toggleReadout:)),
            ("Next session", sel!(nextSession:)),
            ("Tuck / reveal", sel!(toggleTuck:)),
        ] {
            let item = unsafe {
                menu.addItemWithTitle_action_keyEquivalent(
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(""),
                )
            };
            unsafe { item.setTarget(Some(delegate)) };
        }
        let status =
            NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = status.button(mtm) {
            button.setTitle(&NSString::from_str("π"));
        }
        status.setMenu(Some(&menu));
        let mut sessions = Sessions::default();
        if demo {
            sessions.update(0, Snapshot::demo());
        }
        let mut ui = Self {
            pet,
            view,
            card,
            labels,
            card_view: content,
            menu,
            _status: status,
            sessions,
            placement,
            animation: None,
            readout_pinned: readout,
            reveal_timer: None,
            drag_offset: None,
            resize_start: None,
            reveal_armed: true,
            hover_reveal: HoverReveal::default(),
            monitors: Vec::new(),
            edge_watch: None,
        };
        ui.layout_card();
        ui.refresh();
        ui.sync_edge_watch();
        ui.pet.orderFrontRegardless();
        if readout {
            ui.show_readout();
        }
        ui
    }
    fn refresh(&mut self) {
        let activity = self.sessions.activity();
        let previous = self.view.ivars().activity.replace(activity);
        if previous != activity {
            cancel(&mut self.animation);
            self.view.ivars().phase.set(0);
            let celebrate = previous == Activity::Working
                && activity == Activity::Idle
                && self
                    .view
                    .ivars()
                    .sprites
                    .borrow()
                    .as_ref()
                    .is_some_and(|p| p.has_celebration());
            self.view.ivars().celebrate.set(celebrate);
        }
        *self.view.ivars().context.borrow_mut() = self
            .sessions
            .current()
            .and_then(|s| s.snapshot.context.clone());
        let (project, step, usage) = if let Some(session) = self.sessions.current() {
            let step = match activity {
                Activity::Working => session
                    .snapshot
                    .tool
                    .clone()
                    .unwrap_or_else(|| session.snapshot.task.clone()),
                Activity::Waiting => "Awaiting approval".into(),
                Activity::Compacting => activity.label().into(),
                Activity::Error => "Error".into(),
                Activity::Disconnected => "Disconnected".into(),
                Activity::Idle => "Ready".into(),
            };
            let usage = session
                .snapshot
                .context
                .as_ref()
                .map_or("— · —%".into(), |c| {
                    format!("{} · {:.0}%", format_count(c.tokens), c.percent)
                });
            (session.snapshot.project.clone(), step, usage)
        } else {
            ("OMP Pet".into(), "Ready".into(), "— · —%".into())
        };
        let accent = self
            .view
            .ivars()
            .sprites
            .borrow()
            .as_ref()
            .map_or(crate::palette::FALLBACK, |p| p.accent);
        self.card_view.ivars().accent.set(accent);
        self.card_view.setNeedsDisplay(true);
        for (i, (label, text)) in self.labels.iter().zip([project, step, usage]).enumerate() {
            label.setStringValue(&NSString::from_str(&text));
            let tint = if i == 1 {
                color(0.98, 0.98, 0.98, 1.)
            } else {
                color(
                    0.65 + accent[0] as f64 / 255. * 0.35,
                    0.65 + accent[1] as f64 / 255. * 0.35,
                    0.65 + accent[2] as f64 / 255. * 0.35,
                    1.,
                )
            };
            label.setTextColor(Some(&tint));
        }
        if let Some(item) = self.menu.itemAtIndex(0) {
            item.setTitle(&NSString::from_str(if self.readout_pinned {
                "Unpin readout"
            } else {
                "Pin readout"
            }));
        }
        self.view.setNeedsDisplay(true);
        self.sync_animation();
    }
    fn sync_animation(&mut self) {
        let activity = self.sessions.activity();
        let custom = self
            .view
            .ivars()
            .sprites
            .borrow()
            .as_ref()
            .is_some_and(|pack| pack.animated(activity));
        let active = (activity.animates() || custom || self.view.ivars().celebrate.get())
            && !self.view.ivars().tucked.get();
        if active && self.animation.is_none() {
            let index = self.view.ivars().phase.get();
            let interval = self
                .view
                .ivars()
                .sprites
                .borrow()
                .as_ref()
                .map_or(0.33, |pack| {
                    pack.duration(activity, index, self.view.ivars().celebrate.get()) as f64 / 1000.
                });
            self.animation = Some(timer(interval, false, || {
                with_ui(|ui| {
                    ui.animation.take();
                    let next = ui.view.ivars().phase.get().wrapping_add(1);
                    let finished = ui.view.ivars().celebrate.get()
                        && ui
                            .view
                            .ivars()
                            .sprites
                            .borrow()
                            .as_ref()
                            .is_some_and(|pack| next >= pack.celebration_frames());
                    if finished {
                        ui.view.ivars().celebrate.set(false);
                        ui.view.ivars().phase.set(0);
                    } else {
                        ui.view.ivars().phase.set(next);
                    }
                    ui.view.setNeedsDisplay(true);
                    ui.sync_animation();
                })
            }));
        } else if !active {
            if let Some(timer) = self.animation.take() {
                timer.invalidate();
            }
        }
    }
    fn save_preferences(&self) {
        let sprites = self
            .view
            .ivars()
            .sprites
            .borrow()
            .as_ref()
            .map(|pack| pack.path.clone());
        if let Err(e) = preferences::save(&preferences::Preferences {
            placement: Some(self.placement),
            sprites,
        }) {
            eprintln!("Could not save preferences: {e}");
        }
    }
    fn load_sprites(&mut self, path: &std::path::Path) -> Result<(), String> {
        let pack = SpritePack::load(path, self.pet.mtm())?;
        *self.view.ivars().sprites.borrow_mut() = Some(pack);
        cancel(&mut self.animation);
        self.view.ivars().phase.set(0);
        self.view.ivars().celebrate.set(false);
        self.refresh();
        self.save_preferences();
        Ok(())
    }
    fn reload_sprites(&mut self) -> Result<(), String> {
        let path = self
            .view
            .ivars()
            .sprites
            .borrow()
            .as_ref()
            .map(|pack| pack.path.clone());
        if let Some(path) = path {
            self.load_sprites(&path)
        } else {
            Ok(())
        }
    }
    fn install_monitors(&mut self) {
        let mask = NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged;
        let global = RcBlock::new(|_event: NonNull<NSEvent>| with_ui(|ui| ui.pointer_moved()));
        if let Some(m) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global) {
            self.monitors.push(m);
        }
        let local = RcBlock::new(|event: NonNull<NSEvent>| {
            with_ui(|ui| ui.pointer_moved());
            event.as_ptr()
        });
        // SAFETY: returns the unchanged live NSEvent pointer.
        if let Some(m) =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local) }
        {
            self.monitors.push(m);
        }
    }
    // Watch tucked pets and temporary hover reveals, including missed edge transitions.
    fn sync_edge_watch(&mut self) {
        if self.placement.edge.is_some()
            && (self.view.ivars().tucked.get() || self.hover_reveal.active())
            && self.edge_watch.is_none()
        {
            self.edge_watch = Some(timer(0.125, true, || with_ui(|ui| ui.pointer_moved())));
        } else if self.placement.edge.is_none()
            || !(self.view.ivars().tucked.get() || self.hover_reveal.active())
        {
            cancel(&mut self.edge_watch);
        }
    }
    fn physical_screen(&self) -> Rect {
        let center = Point {
            x: self.placement.origin.x + self.placement.size / 2.,
            y: self.placement.origin.y + self.placement.size / 2.,
        };
        let screens = NSScreen::screens(self.pet.mtm());
        screens
            .iter()
            .find(|s| rustrect(s.frame()).contains(center))
            .map_or(self.placement.screen, |s| rustrect(s.frame()))
    }
    fn edge_hit(&self, p: Point) -> bool {
        self.placement.edge.is_some()
            && self
                .placement
                .activation(self.physical_screen())
                .contains(p)
    }
    fn pointer_moved(&mut self) {
        if self.drag_offset.is_some() || self.resize_start.is_some() {
            return;
        }
        let p = cursor();
        if self.view.ivars().tucked.get() {
            if !self.edge_hit(p) {
                self.reveal_armed = true;
                cancel(&mut self.reveal_timer);
                return;
            }
            if !self.reveal_armed {
                return;
            }
            if self.edge_hit(p) && NSEvent::pressedMouseButtons() == 0 {
                if self.reveal_timer.is_none() {
                    self.reveal_timer = Some(timer(0.20, false, || {
                        with_ui(|ui| {
                            ui.reveal_timer.take();
                            if ui.edge_hit(cursor()) {
                                ui.set_tucked(false);
                                ui.hover_reveal.start();
                                ui.sync_edge_watch();
                                ui.pointer_moved();
                            }
                        })
                    }));
                }
            } else {
                cancel(&mut self.reveal_timer);
            }
            return;
        }
        let pet_hit = self.placement.frame(false).contains(p);
        let card_hit = self.card.isVisible() && rustrect(self.card.frame()).contains(p);
        if self.view.ivars().hovered.replace(pet_hit) != pet_hit {
            self.view.setNeedsDisplay(true);
        }
        let inside = pet_hit || card_hit || self.edge_hit(p);
        if self.hover_reveal.should_tuck(
            inside || NSEvent::pressedMouseButtons() != 0,
            self.readout_pinned,
            std::time::Instant::now(),
        ) {
            self.set_tucked(true);
            return;
        }
        let sprite_hit = pet_hit;
        if self.pet.ignoresMouseEvents() == sprite_hit {
            self.pet.setIgnoresMouseEvents(!sprite_hit);
        }
        if inside {
            if pet_hit && !self.card.isVisible() {
                self.refresh();
                self.show_readout();
            }
        } else {
            if !self.readout_pinned && !self.hover_reveal.active() && self.card.isVisible() {
                self.card.orderOut(None);
            }
        }
    }
    fn set_tucked(&mut self, tucked: bool) {
        cancel(&mut self.reveal_timer);
        self.hover_reveal = HoverReveal::default();
        self.view
            .ivars()
            .hovered
            .set(!tucked && self.placement.frame(false).contains(cursor()));
        self.view.ivars().tucked.set(tucked);
        if tucked {
            self.reveal_armed = !self.edge_hit(cursor());
        }
        self.pet.setIgnoresMouseEvents(false);
        let frame = nsrect(if tucked {
            self.placement.tucked_frame(self.physical_screen())
        } else {
            self.placement.frame(false)
        });
        self.pet.setContentMinSize(NSSize::new(0., 0.));
        self.pet.setContentMaxSize(frame.size);
        self.pet.setContentMinSize(frame.size);
        self.pet.setFrame_display(frame, true);
        self.card.orderOut(None);
        self.sync_animation();
        self.sync_edge_watch();
        self.view.setNeedsDisplay(true);
    }
    fn begin_drag(&mut self) {
        self.readout_pinned = false;
        self.set_tucked(false);
        self.card.orderOut(None);
        let p = cursor();
        self.drag_offset = Some(Point {
            x: p.x - self.placement.origin.x,
            y: p.y - self.placement.origin.y,
        });
    }
    fn drag(&mut self) {
        if let Some((start, initial)) = self.resize_start {
            let p = cursor();
            let size = (initial.size + (p.x - start.x - (p.y - start.y)) / 2.).clamp(64., 256.);
            self.placement = initial;
            self.placement.size = size;
            self.placement.origin.y = initial.origin.y + initial.size - size;
            self.placement.align();
            self.set_tucked(false);
            self.layout_card();
            return;
        }
        if let Some(offset) = self.drag_offset {
            let p = cursor();
            self.placement.origin = Point {
                x: p.x - offset.x,
                y: p.y - offset.y,
            };
            self.placement.edge = None;
            self.pet.setFrameOrigin(NSPoint::new(
                self.placement.origin.x,
                self.placement.origin.y,
            ));
        }
    }
    fn end_drag(&mut self) {
        if self.resize_start.take().is_some() {
            self.save_preferences();
            self.refresh();
            return;
        }
        if self.drag_offset.take().is_some() {
            self.placement
                .snap(self.placement.origin, screen_at(cursor(), self.pet.mtm()));
            self.set_tucked(false);
            self.save_preferences();
            self.pointer_moved();
        }
    }
    fn resize_hit(&self, p: Point) -> bool {
        !self.view.ivars().tucked.get()
            && Rect {
                x: self.placement.origin.x + self.placement.size - 14.,
                y: self.placement.origin.y,
                w: 14.,
                h: 14.,
            }
            .contains(p)
    }
    fn begin_resize(&mut self) {
        self.readout_pinned = false;
        self.resize_start = Some((cursor(), self.placement));
        self.card.orderOut(None);
    }
    fn resize_widget(&mut self, size: f64) -> Result<(), String> {
        if !size.is_finite() || !(64. ..=256.).contains(&size) {
            return Err("Pet size must be 64–256".into());
        }
        self.placement.size = size;
        self.placement.align();
        self.set_tucked(false);
        self.layout_card();
        self.refresh();
        self.save_preferences();
        Ok(())
    }
    fn card_scale(&self) -> f64 {
        (self.placement.size / PET_SIZE).max(0.8)
    }
    fn layout_card(&self) {
        let k = self.card_scale();
        let size = NSSize::new(200. * k, 76. * k);
        self.card.setContentMinSize(NSSize::new(0., 0.));
        self.card.setContentMaxSize(size);
        self.card.setContentMinSize(size);
        self.card.setContentSize(size);
        for (label, (y, h, font)) in
            self.labels
                .iter()
                .zip([(55., 14., 10.), (28., 22., 11.), (6., 14., 10.)])
        {
            label.setFrame(rect(8. * k, y * k, 184. * k, h * k));
            label.setFont(Some(&NSFont::systemFontOfSize(font * k)));
        }
    }
    fn show_readout(&self) {
        let p = self.placement.origin;
        let screen = self.placement.screen;
        let k = self.card_scale();
        let gap = 4.;
        let width = 200. * k;
        let height = 76. * k;
        let origin = screen.clamp(
            Point {
                x: p.x + (self.placement.size - width) / 2.,
                y: p.y + self.placement.size + gap,
            },
            width,
            height,
        );
        self.card.setFrameOrigin(NSPoint::new(origin.x, origin.y));
        self.card.orderFrontRegardless();
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = (bool,bool)]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self,_note:&NSNotification) {
            let ui=AppUi::new(self.mtm(),self,self.ivars().0,self.ivars().1);
            UI.with(|slot| *slot.borrow_mut()=Some(ui));
            with_ui(|ui| ui.install_monitors());
        }
        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn screens_changed(&self,_note:&NSNotification) { with_ui(|ui| {
            ui.placement.screen=screen_at(ui.placement.origin,self.mtm()); ui.placement.align();
            let tucked=ui.view.ivars().tucked.get(); ui.set_tucked(tucked); ui.save_preferences();
        }); }
        #[unsafe(method(applicationWillTerminate:))]
        fn will_terminate(&self,_note:&NSNotification) { with_ui(|ui| {
            ui.save_preferences();
            for monitor in ui.monitors.drain(..) { unsafe { NSEvent::removeMonitor(&monitor) }; }
        }); }
    }
    impl Delegate {
        #[unsafe(method(toggleReadout:))]
        fn toggle_readout(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.readout_pinned = !ui.readout_pinned;
            if ui.readout_pinned { ui.show_readout(); } else { ui.card.orderOut(None); }
            ui.refresh();
        }); }
        #[unsafe(method(nextSession:))]
        fn next_session(&self,_sender:Option<&AnyObject>) { with_ui(|ui| { ui.sessions.cycle(); ui.refresh(); }); }
        #[unsafe(method(toggleTuck:))]
        fn toggle_tuck(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            let tucked=!ui.view.ivars().tucked.get();
            if ui.placement.edge.is_none() { ui.placement.edge=Some(crate::geometry::Edge::Right); ui.placement.align(); }
            ui.readout_pinned=false; ui.set_tucked(tucked); ui.save_preferences();
        }); }

    }
);

fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn choose_sprites(mtm: MainThreadMarker) {
    let picker = NSOpenPanel::openPanel(mtm);
    picker.setCanChooseDirectories(true);
    picker.setCanChooseFiles(false);
    picker.setAllowsMultipleSelection(false);
    picker.setMessage(Some(&NSString::from_str(
        "Choose a sprite pack folder containing manifest.json",
    )));
    if picker.runModal() == NSModalResponseOK {
        if let Some(path) = picker.URL().and_then(|url| url.path()) {
            let mut failure = None;
            with_ui(|ui| {
                failure = ui
                    .load_sprites(std::path::Path::new(&path.to_string()))
                    .err()
            });
            if let Some(error) = failure {
                let alert = NSAlert::new(mtm);
                alert.setMessageText(&NSString::from_str("Could not load sprite pack"));
                alert.setInformativeText(&NSString::from_str(&error));
                alert.runModal();
            }
        }
    }
}

pub fn event(event: Event) {
    if matches!(event, Event::Terminate) {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
        return;
    }
    if let Event::Control(Control::ChooseSprites, reply) = event {
        let _ = reply.send("{\"ok\":true,\"picker\":true}".into());
        choose_sprites(MainThreadMarker::new().unwrap());
        return;
    }
    // terminate synchronously invokes applicationWillTerminate; release UI borrows first.
    if let Event::Control(Control::Quit, reply) = event {
        let _ = reply.send("{\"ok\":true}".into());
        return;
    }
    with_ui(|ui| {
        match event {
            Event::Terminate => unreachable!("handled before borrowing UI"),
            Event::Snapshot(connection, snapshot) => {
                ui.sessions.update(connection, snapshot);
            }
            Event::Disconnected(connection) => ui.sessions.disconnect(connection),
            Event::Control(control, reply) => {
                let mut error = None;
                match control {
                    Control::Quit | Control::ChooseSprites => {
                        unreachable!("handled before borrowing UI")
                    }
                    Control::UseCat => {
                        ui.view.ivars().sprites.borrow_mut().take();
                        cancel(&mut ui.animation);
                        ui.view.ivars().celebrate.set(false);
                        ui.refresh();
                        ui.save_preferences();
                    }
                    Control::ShowPet => {
                        ui.readout_pinned = false;
                        ui.set_tucked(false);
                    }
                    Control::Resize { size } => error = ui.resize_widget(size).err(),
                    Control::Readout => {
                        ui.readout_pinned = true;
                        ui.set_tucked(false);
                        ui.show_readout();
                    }
                    Control::Tuck => {
                        if ui.placement.edge.is_none() {
                            ui.placement.edge = Some(crate::geometry::Edge::Right);
                            ui.placement.align();
                        }
                        ui.readout_pinned = false;
                        ui.set_tucked(true);
                        ui.save_preferences();
                    }
                    Control::Reveal => ui.set_tucked(false),
                    Control::ResetPlacement => {
                        ui.placement = Placement::new(screen_at(cursor(), ui.pet.mtm()));
                        ui.set_tucked(false);
                        ui.save_preferences();
                    }
                    Control::NextSession => ui.sessions.cycle(),
                    Control::ReloadSprites => error = ui.reload_sprites().err(),
                    Control::LoadSprites { path } => error = ui.load_sprites(&path).err(),
                    Control::Status => {}
                }
                let status = serde_json::json!({"ok":error.is_none(),"error":error,"pid":std::process::id(),
                    "process_usage":crate::metrics::process_usage(),
                    "tucked":ui.view.ivars().tucked.get(),"hover_revealed":ui.hover_reveal.active(),"grip_visible":ui.view.ivars().hovered.get(),"readout":ui.card.isVisible(),"readout_pinned":ui.readout_pinned,
                    "edge_watch":ui.edge_watch.is_some(),"cursor":cursor(),"animation_running":ui.animation.is_some(),
                    "sprite_viewbox": {"width":ui.placement.size,"height":ui.placement.size},
                    "card_frame":rustrect(ui.card.frame()),"auto_tuck":false,
                    "frame":rustrect(ui.pet.frame()),"activity":ui.sessions.activity(),
                    "sprites":ui.view.ivars().sprites.borrow().as_ref().map(|p|p.path.clone()),
                    "accent":ui.view.ivars().sprites.borrow().as_ref().map(|p|p.accent),
                    "movable":ui.pet.isMovable(),"size_locked":ui.pet.contentMinSize()==ui.pet.contentMaxSize(),
                    "tiling_excluded":ui.pet.collectionBehavior().contains(NSWindowCollectionBehavior::FullScreenDisallowsTiling)});
                let _ = reply.send(status.to_string());
            }
        }
        ui.refresh();
    });
}

pub fn run(demo: bool, readout: bool) {
    let mtm = MainThreadMarker::new().expect("AppKit requires the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let this = Delegate::alloc(mtm).set_ivars((demo, readout));
    let delegate: Retained<Delegate> = unsafe { msg_send![super(this), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
