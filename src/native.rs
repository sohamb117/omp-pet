//! AppKit objects and callbacks stay on the main thread.
use std::{cell::{Cell, RefCell}, ptr::NonNull};
use block2::RcBlock;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol,
    NSPoint, NSRect, NSSize, NSString, NSTimer};
use crate::{geometry::{Placement, Point, Rect, PET_SIZE}, ipc::{Event,Control},
    model::{Activity, ContextUsage, Snapshot}, sessions::Sessions, sprites::SpritePack, preferences};

thread_local! { static UI: RefCell<Option<AppUi>> = const { RefCell::new(None) }; }
fn with_ui(f: impl FnOnce(&mut AppUi)) {
    UI.with(|slot| { if let Some(ui) = slot.borrow_mut().as_mut() { f(ui); } });
}

#[derive(Default)]
struct PetIvars {
    phase: Cell<usize>, activity: Cell<Activity>, context: RefCell<Option<ContextUsage>>,
    tucked: Cell<bool>, celebrate:Cell<bool>, sprites: RefCell<Option<SpritePack>>,
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
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) { with_ui(|ui| ui.begin_drag()); }
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

fn default_sprite_path() -> Option<std::path::PathBuf> {
    let bundle=objc2_foundation::NSBundle::mainBundle();
    let packaged=bundle.resourcePath().map(|p|std::path::PathBuf::from(p.to_string()).join("zorua"));
    packaged.filter(|p|p.join("manifest.json").is_file()).or_else(|| {
        let local=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/zorua");
        local.join("manifest.json").is_file().then_some(local)
    })
}
fn cursor() -> Point { let p=NSEvent::mouseLocation(); Point { x:p.x,y:p.y } }
fn cancel(timer:&mut Option<Retained<NSTimer>>) { if let Some(t)=timer.take() { t.invalidate(); } }
fn screen_at(p:Point,mtm:MainThreadMarker) -> Rect {
    let screens=NSScreen::screens(mtm);
    screens.iter().find(|s| rustrect(s.frame()).contains(p))
        .or_else(|| screens.iter().next()).map(|s| rustrect(s.visibleFrame()))
        .unwrap_or(Rect { x:0.,y:0.,w:1280.,h:720. })
}
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
    if let Some(pack)=ivars.sprites.borrow().as_ref() {
        pack.draw(ivars.activity.get(),ivars.phase.get(),ivars.celebrate.get(),rect(16.,24.,80.,76.));
    } else {
    oval(rect(23.,19.,67.,9.), &color(0.,0.,0.,0.12));
    rounded(rect(26.,27.,62.,58.),23.,&ink);
    oval(rect(26.,70.,23.,25.),&ink); oval(rect(65.,70.,23.,25.),&ink);
    rounded(rect(30.,31.,54.,50.),20.,&mint);
    oval(rect(30.,74.,15.,17.),&mint); oval(rect(69.,74.,15.,17.),&mint);
    oval(rect(41.,54.,6.,9.),&ink); oval(rect(67.,54.,6.,9.),&ink);
    rounded(rect(52.,46.,10.,3.),1.5,&ink);
    oval(rect(34.,47.,9.,5.),&color(0.96,0.62,0.64,0.75));
    oval(rect(73.,47.,9.,5.),&color(0.96,0.62,0.64,0.75));
    }
    rounded(rect(29.,13.,54.,2.),1.,&color(0.10,0.13,0.18,0.35));
    if let Some(c)=ivars.context.borrow().as_ref() {
        let rgb=ivars.sprites.borrow().as_ref().map_or(crate::palette::FALLBACK,|pack|pack.accent);
        let fill=color(rgb[0] as f64/255.,rgb[1] as f64/255.,rgb[2] as f64/255.,1.);
        rounded(rect(29.,13.,54.*c.percent/100.,2.),1.,&fill);
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
    panel.setMovable(false); // All dragging is handled by PetView, never the window server.
    panel.setMovableByWindowBackground(false);
    panel.setExcludedFromWindowsMenu(true);
    panel.setContentMinSize(bounds.size); panel.setContentMaxSize(bounds.size);
    panel.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::FullScreenAuxiliary | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::Stationary | NSWindowCollectionBehavior::FullScreenDisallowsTiling);
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
    reveal_timer:Option<Retained<NSTimer>>, hide_timer:Option<Retained<NSTimer>>,
    drag_offset:Option<Point>, monitors:Vec<Retained<AnyObject>>,
}
impl AppUi {
    fn new(mtm:MainThreadMarker,delegate:&Delegate,demo:bool,readout:bool) -> Self {
        let screen=rustrect(NSScreen::mainScreen(mtm).expect("a screen is required").visibleFrame());
        let prefs=preferences::load();
        let mut placement=prefs.placement.unwrap_or_else(|| Placement::new(screen));
        placement.screen=screen_at(Point { x:placement.origin.x+PET_SIZE/2.,y:placement.origin.y+PET_SIZE/2. },mtm);
        placement.align();
        let pet=panel(mtm,nsrect(placement.frame(false)),"OMP Pet");
        let view=PetView::new(mtm); pet.setContentView(Some(&view));
        view.setAccessibilityElement(true);
        view.setAccessibilityRole(Some(unsafe { NSAccessibilityImageRole }));
        view.setAccessibilityLabel(Some(&NSString::from_str("OMP Pet desktop companion")));
        pet.setAcceptsMouseMovedEvents(true);
        if let Some(path)=std::env::var_os("OMP_PET_SPRITES").map(std::path::PathBuf::from).or(prefs.sprites).or_else(default_sprite_path) {
            match SpritePack::load(&path,mtm) {
                Ok(pack) => *view.ivars().sprites.borrow_mut()=Some(pack),
                Err(e) => eprintln!("Could not load sprite pack: {e}"),
            }
        }
        let card=panel(mtm,rect(0.,0.,320.,190.),"OMP Pet — task readout");
        let content:Retained<CardView>=unsafe { msg_send![CardView::alloc(mtm),initWithFrame:rect(0.,0.,320.,190.)] };
        let mut labels=Vec::new();
        for (y,height,size) in [(157.,18.,11.),(104.,44.,16.),(76.,18.,12.),(48.,18.,12.),(20.,18.,11.)] {
            let label=NSTextField::labelWithString(&NSString::from_str(""),mtm);
            label.setFrame(rect(18.,y,284.,height));
            label.setFont(Some(&NSFont::systemFontOfSize(size)));
            label.setTextColor(Some(&color(0.90,0.94,0.96,1.)));
            label.setMaximumNumberOfLines(if height > 20. { 2 } else { 1 });
            content.addSubview(&label); labels.push(label);
        }
        card.setContentView(Some(&content)); card.setHasShadow(true);
        let menu=NSMenu::new(mtm); menu.setAutoenablesItems(false);
        for (title,action) in [("Show task readout",sel!(toggleReadout:)),("Next session",sel!(nextSession:)),
            ("Tuck / reveal",sel!(toggleTuck:)),("Choose sprite pack…",sel!(chooseSprites:)),("Reload sprites",sel!(reloadSprites:)),
            ("Use built-in sprite",sel!(clearSprites:)),("Reset placement",sel!(resetPlacement:)),("Quit OMP Pet",sel!(quit:))] {
            let item=unsafe { menu.addItemWithTitle_action_keyEquivalent(&NSString::from_str(title),Some(action),&NSString::from_str("")) };
            unsafe { item.setTarget(Some(delegate)) };
        }
        let status=NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button)=status.button(mtm) { button.setTitle(&NSString::from_str("π")); }
        status.setMenu(Some(&menu));
        let mut sessions=Sessions::default();
        if demo { sessions.update(0,Snapshot::demo()); }
        let mut ui=Self { pet,view,card,labels,menu,_status:status,sessions,placement,animation:None,readout_pinned:readout,
            reveal_timer:None,hide_timer:None,drag_offset:None,monitors:Vec::new() };
        ui.refresh(); ui.pet.orderFrontRegardless();
        if readout { ui.show_readout(); }
        ui
    }
    fn refresh(&mut self) {
        let activity=self.sessions.activity();
        let previous=self.view.ivars().activity.replace(activity);
        if previous != activity {
            cancel(&mut self.animation); self.view.ivars().phase.set(0);
            let celebrate=previous==Activity::Working && activity==Activity::Idle
                && self.view.ivars().sprites.borrow().as_ref().is_some_and(|p|p.has_celebration());
            self.view.ivars().celebrate.set(celebrate);
        }
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
        let activity=self.sessions.activity();
        let custom=self.view.ivars().sprites.borrow().as_ref().is_some_and(|pack|pack.animated(activity));
        let active=(activity.animates() || custom || self.view.ivars().celebrate.get()) && !self.view.ivars().tucked.get();
        if active && self.animation.is_none() {
            let index=self.view.ivars().phase.get();
            let interval=self.view.ivars().sprites.borrow().as_ref().map_or(0.33,|pack|
                pack.duration(activity,index,self.view.ivars().celebrate.get()) as f64/1000.);
            self.animation=Some(timer(interval,false,|| with_ui(|ui| {
                ui.animation.take();
                let next=ui.view.ivars().phase.get().wrapping_add(1);
                let finished=ui.view.ivars().celebrate.get() && ui.view.ivars().sprites.borrow().as_ref()
                    .is_some_and(|pack|next>=pack.celebration_frames());
                if finished { ui.view.ivars().celebrate.set(false); ui.view.ivars().phase.set(0); }
                else { ui.view.ivars().phase.set(next); }
                ui.view.setNeedsDisplay(true); ui.sync_animation();
            })));
        } else if !active {
            if let Some(timer)=self.animation.take() { timer.invalidate(); }
        }
    }
    fn save_preferences(&self) {
        let sprites=self.view.ivars().sprites.borrow().as_ref().map(|pack|pack.path.clone());
        if let Err(e)=preferences::save(&preferences::Preferences { placement:Some(self.placement),sprites }) {
            eprintln!("Could not save preferences: {e}");
        }
    }
    fn load_sprites(&mut self,path:&std::path::Path) -> Result<(),String> {
        let pack=SpritePack::load(path,self.pet.mtm())?;
        *self.view.ivars().sprites.borrow_mut()=Some(pack);
        cancel(&mut self.animation); self.refresh(); self.save_preferences(); Ok(())
    }
    fn reload_sprites(&mut self) {
        let path=self.view.ivars().sprites.borrow().as_ref().map(|pack|pack.path.clone());
        if let Some(path)=path { if let Err(e)=self.load_sprites(&path) { eprintln!("Could not reload sprites: {e}"); } }
    }
    fn install_monitors(&mut self) {
        let mask=NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged;
        let global=RcBlock::new(|_event:NonNull<NSEvent>| with_ui(|ui| ui.pointer_moved()));
        if let Some(m)=NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask,&global) { self.monitors.push(m); }
        let local=RcBlock::new(|event:NonNull<NSEvent>| { with_ui(|ui| ui.pointer_moved()); event.as_ptr() });
        // SAFETY: returns the unchanged live NSEvent pointer.
        if let Some(m)=unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask,&local) } { self.monitors.push(m); }
    }
    fn pointer_moved(&mut self) {
        if self.drag_offset.is_some() { return; }
        let p=cursor();
        if self.view.ivars().tucked.get() {
            if self.placement.frame(true).contains(p) && NSEvent::pressedMouseButtons()==0 {
                if self.reveal_timer.is_none() {
                    self.reveal_timer=Some(timer(0.20,false,|| with_ui(|ui| {
                        ui.reveal_timer.take();
                        if ui.placement.frame(true).contains(cursor()) {
                            ui.set_tucked(false); ui.pointer_moved();
                        }
                    })));
                }
            } else { cancel(&mut self.reveal_timer); }
            return;
        }
        let pet_hit=self.placement.frame(false).contains(p);
        let card_hit=self.card.isVisible() && rustrect(self.card.frame()).contains(p);
        // Empty space around the sprite lets clicks reach the application below it.
        let local=Point { x:p.x-self.placement.origin.x,y:p.y-self.placement.origin.y };
        let sprite_hit=Rect { x:16.,y:24.,w:80.,h:76. }.contains(local)
            || Rect { x:77.,y:77.,w:32.,h:23. }.contains(local)
            || Rect { x:29.,y:12.,w:54.,h:5. }.contains(local);
        self.pet.setIgnoresMouseEvents(!sprite_hit);
        if pet_hit || card_hit {
            cancel(&mut self.hide_timer);
            if pet_hit && !self.card.isVisible() { self.refresh(); self.show_readout(); }
        } else {
            if !self.readout_pinned { self.card.orderOut(None); }
            if self.placement.edge.is_some() && self.hide_timer.is_none() && !self.readout_pinned {
                self.hide_timer=Some(timer(0.8,false,|| with_ui(|ui| {
                    ui.hide_timer.take();
                    let p=cursor();
                    if !ui.placement.frame(false).contains(p)
                        && !(ui.card.isVisible() && rustrect(ui.card.frame()).contains(p)) { ui.set_tucked(true); }
                })));
            }
        }
    }
    fn set_tucked(&mut self,tucked:bool) {
        cancel(&mut self.reveal_timer); cancel(&mut self.hide_timer);
        self.view.ivars().tucked.set(tucked);
        self.pet.setIgnoresMouseEvents(false);
        let frame=nsrect(self.placement.frame(tucked));
        self.pet.setContentMinSize(frame.size); self.pet.setContentMaxSize(frame.size);
        self.pet.setFrame_display(frame,true);
        self.card.orderOut(None); self.sync_animation(); self.view.setNeedsDisplay(true);
    }
    fn begin_drag(&mut self) {
        self.set_tucked(false); self.card.orderOut(None);
        let p=cursor(); self.drag_offset=Some(Point { x:p.x-self.placement.origin.x,y:p.y-self.placement.origin.y });
    }
    fn drag(&mut self) {
        if let Some(offset)=self.drag_offset {
            let p=cursor(); self.placement.origin=Point { x:p.x-offset.x,y:p.y-offset.y };
            self.placement.edge=None;
            self.pet.setFrameOrigin(NSPoint::new(self.placement.origin.x,self.placement.origin.y));
        }
    }
    fn end_drag(&mut self) {
        if self.drag_offset.take().is_some() {
            self.placement.snap(self.placement.origin,screen_at(cursor(),self.pet.mtm()));
            self.set_tucked(false); self.save_preferences(); self.pointer_moved();
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
        #[unsafe(method(chooseSprites:))]
        fn choose_sprites(&self,_sender:Option<&AnyObject>) {
            let picker=NSOpenPanel::openPanel(self.mtm());
            picker.setCanChooseDirectories(true); picker.setCanChooseFiles(false);
            picker.setAllowsMultipleSelection(false);
            picker.setMessage(Some(&NSString::from_str("Choose a sprite pack folder containing manifest.json")));
            if picker.runModal()==NSModalResponseOK {
                if let Some(path)=picker.URL().and_then(|url|url.path()) {
                    let mut failure=None;
                    with_ui(|ui| { failure=ui.load_sprites(std::path::Path::new(&path.to_string())).err(); });
                    if let Some(error)=failure {
                        let alert=NSAlert::new(self.mtm()); alert.setMessageText(&NSString::from_str("Could not load sprite pack"));
                        alert.setInformativeText(&NSString::from_str(&error)); alert.runModal();
                    }
                }
            }
        }
        #[unsafe(method(reloadSprites:))]
        fn reload_sprites(&self,_sender:Option<&AnyObject>) { with_ui(|ui|ui.reload_sprites()); }
        #[unsafe(method(clearSprites:))]
        fn clear_sprites(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.view.ivars().sprites.borrow_mut().take(); cancel(&mut ui.animation); ui.refresh(); ui.save_preferences();
        }); }
        #[unsafe(method(toggleReadout:))]
        fn toggle_readout(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.readout_pinned=!ui.readout_pinned;
            if ui.readout_pinned { ui.show_readout(); } else { ui.card.orderOut(None); }
        }); }
        #[unsafe(method(nextSession:))]
        fn next_session(&self,_sender:Option<&AnyObject>) { with_ui(|ui| { ui.sessions.cycle(); ui.refresh(); }); }
        #[unsafe(method(toggleTuck:))]
        fn toggle_tuck(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            let tucked=!ui.view.ivars().tucked.get();
            if ui.placement.edge.is_none() { ui.placement.edge=Some(crate::geometry::Edge::Right); ui.placement.align(); }
            ui.readout_pinned=false; ui.set_tucked(tucked); ui.save_preferences();
        }); }
        #[unsafe(method(resetPlacement:))]
        fn reset_placement(&self,_sender:Option<&AnyObject>) { with_ui(|ui| {
            ui.placement=Placement::new(rustrect(NSScreen::mainScreen(self.mtm()).unwrap().visibleFrame()));
            ui.set_tucked(false); ui.refresh(); ui.save_preferences();
        }); }
        #[unsafe(method(quit:))]
        fn quit(&self,_sender:Option<&AnyObject>) { NSApplication::sharedApplication(self.mtm()).terminate(None); }
    }
);

pub fn event(event:Event) {
    // terminate synchronously invokes applicationWillTerminate; release UI borrows first.
    if matches!(event,Event::Control(Control::Quit)) {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap()).terminate(None);
        return;
    }
    with_ui(|ui| {
        match event {
            Event::Snapshot(connection,snapshot) => { ui.sessions.update(connection,snapshot); }
            Event::Disconnected(connection) => ui.sessions.disconnect(connection),
            Event::Control(control) => match control {
                Control::Quit => unreachable!("handled before borrowing UI"),
                Control::Readout => { ui.readout_pinned=true; ui.set_tucked(false); ui.show_readout(); }
                Control::Tuck => {
                    if ui.placement.edge.is_none() { ui.placement.edge=Some(crate::geometry::Edge::Right); ui.placement.align(); }
                    ui.readout_pinned=false; ui.set_tucked(true);
                }
                Control::Reveal => ui.set_tucked(false),
                Control::ResetPlacement => {
                    ui.placement=Placement::new(screen_at(cursor(),ui.pet.mtm())); ui.set_tucked(false);
                }
                Control::NextSession => ui.sessions.cycle(),
                Control::ReloadSprites => ui.reload_sprites(),
            },
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
