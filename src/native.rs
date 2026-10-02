//! AppKit stays on the main thread; the panel never takes keyboard focus.
use std::cell::{Cell, OnceCell};
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSBackingStoreType, NSBezierPath, NSColor, NSPanel, NSScreen, NSView,
    NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize};

pub const PET_SIZE: f64 = 112.0;

#[derive(Default)]
struct PetIvars { phase: Cell<u8> }

define_class!(
    // SAFETY: NSView subclass, used exclusively on the main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = PetIvars]
    pub struct PetView;
    unsafe impl NSObjectProtocol for PetView {}
    impl PetView {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            draw_pet(self.ivars().phase.get());
        }
    }
);

impl PetView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PetIvars::default());
        unsafe { msg_send![super(this), initWithFrame: rect(0., 0., PET_SIZE, PET_SIZE)] }
    }
}

define_class!(
    // SAFETY: NSPanel subclass; prevents even clicks from stealing keyboard focus.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    pub struct PetPanel;
    unsafe impl NSObjectProtocol for PetPanel {}
    impl PetPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

pub fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x,y), NSSize::new(w,h))
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

fn draw_pet(_phase: u8) {
    let ink = color(0.10, 0.13, 0.18, 1.);
    let mint = color(0.64, 0.89, 0.77, 1.);
    oval(rect(23., 15., 67., 9.), &color(0., 0., 0., 0.12));
    rounded(rect(26., 23., 62., 58.), 23., &ink);
    oval(rect(26., 66., 23., 25.), &ink);
    oval(rect(65., 66., 23., 25.), &ink);
    rounded(rect(30., 27., 54., 50.), 20., &mint);
    oval(rect(30., 70., 15., 17.), &mint);
    oval(rect(69., 70., 15., 17.), &mint);
    oval(rect(41., 50., 6., 9.), &ink);
    oval(rect(67., 50., 6., 9.), &ink);
    rounded(rect(52., 42., 10., 3.), 1.5, &ink);
    oval(rect(34., 43., 9., 5.), &color(0.96, 0.62, 0.64, 0.75));
    oval(rect(73., 43., 9., 5.), &color(0.96, 0.62, 0.64, 0.75));
}

pub fn panel(mtm: MainThreadMarker, bounds: NSRect) -> Retained<PetPanel> {
    let panel: Retained<PetPanel> = unsafe {
        msg_send![PetPanel::alloc(mtm), initWithContentRect: bounds,
            styleMask: NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            backing: NSBackingStoreType::Buffered, defer: false]
    };
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setLevel(3); // NSFloatingWindowLevel, above ordinary application windows.
    panel.setHidesOnDeactivate(false);
    panel.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary
        | NSWindowCollectionBehavior::IgnoresCycle);
    panel
}

#[derive(Default)]
struct DelegateIvars { panel: OnceCell<Retained<PetPanel>> }

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _note: &NSNotification) {
            let mtm = self.mtm();
            let screen = NSScreen::mainScreen(mtm).expect("a screen is required").visibleFrame();
            let pet = panel(mtm, rect(screen.origin.x + screen.size.width - PET_SIZE - 24.,
                screen.origin.y + 40., PET_SIZE, PET_SIZE));
            pet.setContentView(Some(&PetView::new(mtm)));
            pet.orderFrontRegardless();
            self.ivars().panel.set(pet).ok();
        }
    }
);

pub fn run() {
    let mtm = MainThreadMarker::new().expect("AppKit requires the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let this = Delegate::alloc(mtm).set_ivars(DelegateIvars::default());
    let delegate: Retained<Delegate> = unsafe { msg_send![super(this), init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
