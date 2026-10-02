//! AppKit objects and callbacks stay on the main thread.
use std::{cell::{Cell, RefCell}, ptr::NonNull};
use block2::RcBlock;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, NSString, NSTimer};
use crate::{geometry::{Placement, Point, Rect, PET_SIZE}, ipc::Event,
    model::{Activity, ContextUsage, Snapshot}, sessions::Sessions};

thread_local! { static UI: RefCell<Option<AppUi>> = const { RefCell::new(None) }; }
fn with_ui(f: impl FnOnce(&mut AppUi)) {
    UI.with(|slot| { if let Some(ui) = slot.borrow_mut().as_mut() { f(ui); } });
}

#[derive(Default)]
struct PetIvars {
    phase: Cell<u8>, activity: Cell<Activity>, context: RefCell<Option<ContextUsage>>,
    tucked: Cell<bool>,
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
                rounded(self.bounds(), 4., &color(0.45, 0.77, 0.64, 0.9));
            } else {
                draw_pet(self.ivars());
            }
        }
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
        unsafe { msg_send![super(this), initWithFrame: nsrect(Rect { x: 0., y: 0., w: PET_SIZE, h: PET_SIZE })] }
    }
}

define_class!(
    // SAFETY: NSPanel subclass. Pet and hover card cannot become keyboard targets.
    #[unsafe(super = NSPanel)]
    #[thread_kind = MainThreadOnly]
    struct PetPanel;
    unsafe impl NSObjectProtocol for PetPanel {}
    impl PetPanel {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool { false }
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool { false }
    }
);

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct CardView;
    unsafe impl NSObjectProtocol for CardView {}
    impl CardView {
        #[unsafe(method(drawRect:))]
        fn draw(&self, _rect: NSRect) {
            rounded(self.bounds(), 16., &color(0.07, 0.10, 0.14, 0.98));
        }
    }
);

fn nsrect(r: Rect) -> NSRect { NSRect::new(NSPoint::new(r.x,r.y), NSSize::new(r.w,r.h)) }
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect { nsrect(Rect { x,y,w,h }) }
fn rustrect(r: NSRect) -> Rect { Rect { x:r.origin.x, y:r.origin.y, w:r.size.width, h:r.size.height } }
fn color(r:f64,g:f64,b:f64,a:f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(r,g,b,a)
}
fn rounded(bounds:NSRect,radius:f64,fill:&NSColor) {
    fill.setFill();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds,radius,radius).fill();
}
fn oval(bounds:NSRect,fill:&NSColor) {
    fill.setFill(); NSBezierPath::bezierPathWithOvalInRect(bounds).fill();
}
fn draw_pet(ivars:&PetIvars) {
    let ink=color(0.10,0.13,0.18,1.); let mint=color(0.64,0.89,0.77,1.);
    oval(rect(23.,19.,67.,9.), &color(0.,0.,0.,0.12));
    rounded(rect(26.,27.,62.,58.),23.,&ink);
    oval(rect(26.,70.,23.,25.),&ink); oval(rect(65.,70.,23.,25.),&ink);
    rounded(rect(30.,31.,54.,50.),20.,&mint);
    oval(rect(30.,74.,15.,17.),&mint); oval(rect(69.,74.,15.,17.),&mint);
    oval(rect(41.,54.,6.,9.),&ink); oval(rect(67.,54.,6.,9.),&ink);
    rounded(rect(52.,46.,10.,3.),1.5,&ink);
    oval(rect(34.,47.,9.,5.),&color(0.96,0.62,0.64,0.75));
    oval(rect(73.,47.,9.,5.),&color(0.96,0.62,0.64,0.75));
    rounded(rect(29.,12.,54.,5.),2.5,&color(0.10,0.13,0.18,0.35));
    if let Some(c)=ivars.context.borrow().as_ref() {
        let fill=if c.percent >= 85. { color(0.96,0.66,0.35,1.) } else { mint.clone() };
        rounded(rect(29.,12.,54.*c.percent/100.,5.),2.5,&fill);
    }
    let activity=ivars.activity.get();
    if activity != Activity::Idle {
        rounded(rect(77.,80.,32.,20.),7.,&ink);
        rounded(rect(80.,77.,7.,7.),2.,&ink);
        let tint=match activity {
            Activity::Error => color(1.,0.45,0.45,1.),
            Activity::Waiting => color(1.,0.76,0.39,1.),
            Activity::Disconnected => color(0.55,0.60,0.66,1.),
            _ => mint,
        };
        for i in 0..3 {
            let alpha=if activity.animates() && i != ivars.phase.get()%3 { 0.3 } else { 1. };
            oval(rect(83.+i as f64*8.,87.,4.,4.),&tint.colorWithAlphaComponent(alpha));
        }
    }
}
fn panel(mtm:MainThreadMarker,bounds:NSRect,title:&str) -> Retained<PetPanel> {
    let panel:Retained<PetPanel>=unsafe { msg_send![PetPanel::alloc(mtm),
        initWithContentRect:bounds, styleMask:NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        backing:NSBackingStoreType::Buffered, defer:false] };
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setTitle(&NSString::from_str(title));
    panel.setOpaque(false); panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false); panel.setLevel(3); panel.setHidesOnDeactivate(false);
    panel.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::IgnoresCycle);
    panel
}

fn timer(seconds:f64,repeats:bool,f:impl Fn()+'static) -> Retained<NSTimer> {
    let block=RcBlock::new(move |_timer:NonNull<NSTimer>| f());
    // SAFETY: the block runs on the main run loop and captures no thread-bound pointers.
    let timer=unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(seconds,repeats,&block) };
    timer.setTolerance(if repeats { seconds*0.1 } else { 0.02 });
    timer
}

struct AppUi {
    pet:Retained<PetPanel>, view:Retained<PetView>, card:Retained<PetPanel>, labels:Vec<Retained<NSTextField>>,
    menu:Retained<NSMenu>, _status:Retained<NSStatusItem>, sessions:Sessions,
    placement:Placement, animation:Option<Retained<NSTimer>>, readout_pinned:bool,
}
impl AppUi {
    fn new(mtm:MainThreadMarker,delegate:&Delegate,demo:bool,readout:bool) -> Self {
        let screen=rustrect(NSScreen::mainScreen(mtm).expect("a screen is required").visibleFrame());
        let placement=Placement::new(screen);
        let pet=panel(mtm,nsrect(placement.frame(false)),"OMP Pet");
        let view=PetView::new(mtm); pet.setContentView(Some(&view));
        pet.setAcceptsMouseMovedEvents(true);
        let card=panel(mtm,rect(0.,0.,320.,190.),"OMP Pet — task readout");
        let content:Retained<CardView>=unsafe { msg_send![CardView::alloc(mtm),initWithFrame:rect(0.,0.,320.,190.)] };
        let mut labels=Vec::new();
        for (y,height,size) in [(157.,18.,11.),(104.,44.,16.),(76.,18.,12.),(48.,18.,12.),(20.,18.,11.)] {
            let label=NSTextField::labelWithString(&NSString::from_str(""),mtm);
            label.setFrame(rect(18.,y,284.,height));
            label.setFont(Some(&NSFont::systemFontOfSize(size)));
            label.setTextColor(Some(&color(0.90,0.94,0.96,1.)));
            label.setMaximumNumberOfLines(if height > 20. { 2 } else { 1 });
            unsafe { content.addSubview(&label) }; labels.push(label);
        }
        card.setContentView(Some(&content)); card.setHasShadow(true);
        let menu=NSMenu::new(mtm); menu.setAutoenablesItems(false);
        for (title,action) in [("Show task readout",sel!(toggleReadout:)),("Next session",sel!(nextSession:)),
            ("Tuck / reveal",sel!(toggleTuck:)),("Reset placement",sel!(resetPlacement:)),("Quit OMP Pet",sel!(quit:))] {
            let item=unsafe { menu.addItemWithTitle_action_keyEquivalent(&NSString::from_str(title),Some(action),&NSString::from_str("")) };
            unsafe { item.setTarget(Some(delegate)) };
        }
        let status=NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button)=status.button(mtm) { button.setTitle(&NSString::from_str("π")); }
        status.setMenu(Some(&menu));
        let mut sessions=Sessions::default();
        if demo { sessions.update(0,Snapshot::demo()); }
        let mut ui=Self { pet,view,card,labels,menu,_status:status,sessions,placement,animation:None,readout_pinned:readout };
        ui.refresh(); ui.pet.orderFrontRegardless();
        if readout { ui.show_readout(); }
        ui
    }
    fn refresh(&mut self) {
        let activity=self.sessions.activity(); self.view.ivars().activity.set(activity);
        *self.view.ivars().context.borrow_mut()=self.sessions.current().and_then(|s| s.snapshot.context.clone());
        let (project,task,tool,context,footer)=if let Some(s)=self.sessions.current() {
            (format!("{} · {}",s.snapshot.project,activity.label()),s.snapshot.task.clone(),
                s.snapshot.tool.clone().unwrap_or_else(|| activity.label().into()),
                s.snapshot.context.as_ref().map_or("Context unknown".into(),|c|
                    format!("Context ≈ {:.0}% · {} / {} tokens",c.percent,c.tokens,c.window)),
                format!("{} session(s) · last activity {}s ago",self.sessions.entries.len(),s.last_activity.elapsed().as_secs()))
        } else {
            ("OMP PET · READY".into(),"Your desktop companion".into(),"Waiting for an oh-my-pi session".into(),
                "Context unknown".into(),"Drag to an edge to tuck away · right-click for options".into())
        };
        for (label,text) in self.labels.iter().zip([project,task,tool,context,footer]) {
            label.setStringValue(&NSString::from_str(&text));
        }
        self.view.setNeedsDisplay(true);
        self.sync_animation();
    }
    fn sync_animation(&mut self) {
        let active=self.sessions.activity().animates() && !self.view.ivars().tucked.get();
        if active && self.animation.is_none() {
            self.animation=Some(timer(0.33,true,|| with_ui(|ui| {
                let p=ui.view.ivars().phase.get(); ui.view.ivars().phase.set((p+1)%3);
                ui.view.setNeedsDisplay(true);
            })));
        } else if !active {
            if let Some(timer)=self.animation.take() { timer.invalidate(); }
        }
    }
    fn show_readout(&self) {
        let p=self.placement.origin; let screen=self.placement.screen;
        let x=if p.x-328. >= screen.x { p.x-328. } else { p.x+PET_SIZE+8. };
        let origin=screen.clamp(Point { x,y:p.y+PET_SIZE-190. },320.,190.);
        self.card.setFrameOrigin(NSPoint::new(origin.x,origin.y)); self.card.orderFrontRegardless();
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
        }
    }
    impl Delegate {
        #[unsafe(method(toggleReadout:))]
        fn toggle_readout(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.readout_pinned=!ui.readout_pinned;
            if ui.readout_pinned { ui.show_readout(); } else { ui.card.orderOut(None); }
        }); }
        #[unsafe(method(nextSession:))]
        fn next_session(&self,_sender:Option<&AnyObject>) { with_ui(|ui| { ui.sessions.cycle(); ui.refresh(); }); }
        #[unsafe(method(toggleTuck:))]
        fn toggle_tuck(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            let tucked=!ui.view.ivars().tucked.get(); ui.view.ivars().tucked.set(tucked);
            if ui.placement.edge.is_none() { ui.placement.edge=Some(crate::geometry::Edge::Right); ui.placement.align(); }
            ui.pet.setFrame_display(nsrect(ui.placement.frame(tucked)),true);
            ui.card.orderOut(None); ui.refresh();
        }); }
        #[unsafe(method(resetPlacement:))]
        fn reset_placement(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.placement=Placement::new(rustrect(NSScreen::mainScreen(self.mtm()).unwrap().visibleFrame()));
            ui.view.ivars().tucked.set(false); ui.pet.setFrame_display(nsrect(ui.placement.frame(false)),true); ui.refresh();
        }); }
        #[unsafe(method(quit:))]
        fn quit(&self,_sender:Option<&AnyObject>) { unsafe { NSApplication::sharedApplication(self.mtm()).terminate(None) }; }
    }
);

pub fn event(event:Event) {
    with_ui(|ui| {
        match event {
            Event::Snapshot(connection,snapshot) => { ui.sessions.update(connection,snapshot); }
            Event::Disconnected(connection) => ui.sessions.disconnect(connection),
        }
        ui.refresh();
    });
}

pub fn run(demo:bool,readout:bool) {
    let mtm=MainThreadMarker::new().expect("AppKit requires the main thread");
    let app=NSApplication::sharedApplication(mtm); app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let this=Delegate::alloc(mtm).set_ivars((demo,readout));
    let delegate:Retained<Delegate>=unsafe { msg_send![super(this),init] };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate))); app.run();
}
