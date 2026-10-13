//! Keeps the traffic lights in the window bar while the window is in macOS
//! full screen, the way WebStorm does. In full screen AppKit moves a window's
//! title bar into a strip of its own that slides down with the menu bar and
//! covers the top of the app. So while full screen this hides that strip, so
//! only the menu bar drops down, and puts a second, real set of buttons (from
//! `+[NSWindow standardWindowButton:forStyleMask:]`) into the window itself, at
//! the spot tauri.macos.conf.json gives the originals. Being the system's own
//! buttons, the green one still offers Exit Full Screen and tiling on hover.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{
    define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSButton, NSEvent, NSResponder, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow, NSWindowButton,
    NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
    NSWindowStyleMask, NSWindowWillExitFullScreenNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSPoint, NSRect, NSSize};
use tauri::{Runtime, Window};

/// `trafficLightPosition.x` in tauri.macos.conf.json.
const LEFT: f64 = 16.0;
/// `--chrome-h`: the buttons are centred on the window bar.
const BAR_HEIGHT: f64 = 44.0;
/// AppKit's gap from one button's left edge to the next, if it can't be read.
const DEFAULT_SPACING: f64 = 20.0;

define_class!(
    // SAFETY: NSView has no subclassing requirements and this has no Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AugryTrafficLights"]
    #[ivars = Cell<bool>]
    struct TrafficLights;

    impl TrafficLights {
        // A window button asks its superview whether the pointer is over the
        // group to decide whether to draw its ×, – and arrows glyphs.
        #[unsafe(method(_mouseInGroup:))]
        fn mouse_in_group(&self, _button: &AnyObject) -> bool {
            self.ivars().get()
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.set_hovered(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.set_hovered(false);
        }
    }
);

impl TrafficLights {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(false));
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let owner: &AnyObject = &this;
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                NSTrackingAreaOptions::MouseEnteredAndExited
                    | NSTrackingAreaOptions::ActiveAlways
                    | NSTrackingAreaOptions::InVisibleRect,
                Some(owner),
                None,
            )
        };
        this.addTrackingArea(&area);
        this
    }

    fn set_hovered(&self, hovered: bool) {
        self.ivars().set(hovered);
        for button in self.subviews() {
            button.setNeedsDisplay(true);
        }
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this has no Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AugryFullScreenObserver"]
    struct FullScreenObserver;

    impl FullScreenObserver {
        #[unsafe(method(didEnterFullScreen:))]
        fn did_enter_full_screen(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                enter(&window, self.mtm());
            }
        }

        #[unsafe(method(leaveFullScreen:))]
        fn leave_full_screen(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                leave(&window);
            }
        }
    }
);

fn window_of(notification: &NSNotification) -> Option<Retained<NSWindow>> {
    notification.object()?.downcast::<NSWindow>().ok()
}

thread_local! {
    /// AppKit is main thread only, so the observer and the full screen
    /// buttons live there. The notification center doesn't retain observers.
    static OBSERVER: RefCell<Option<Retained<FullScreenObserver>>> = const { RefCell::new(None) };
    static INSTALLED: RefCell<Option<Retained<TrafficLights>>> = const { RefCell::new(None) };
}

/// Starts following the window in and out of full screen. Call once, at setup.
pub fn watch<R: Runtime>(window: &Window<R>) {
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(handle) = target.ns_window() else {
            return;
        };
        // SAFETY: tauri hands back the live NSWindow, and this is its thread.
        let ns_window: &NSWindow = unsafe { &*handle.cast() };

        let observer: Retained<FullScreenObserver> =
            unsafe { msg_send![FullScreenObserver::alloc(mtm), init] };
        let center = NSNotificationCenter::defaultCenter();
        let object: &AnyObject = ns_window;
        let events = [
            (sel!(didEnterFullScreen:), unsafe {
                NSWindowDidEnterFullScreenNotification
            }),
            // Give the title bar back as the exit starts, so the transition
            // shows it; did-exit makes sure of it whatever happened between.
            (sel!(leaveFullScreen:), unsafe {
                NSWindowWillExitFullScreenNotification
            }),
            (sel!(leaveFullScreen:), unsafe {
                NSWindowDidExitFullScreenNotification
            }),
        ];
        for (selector, name) in events {
            unsafe {
                center.addObserver_selector_name_object(
                    &observer,
                    selector,
                    Some(name),
                    Some(object),
                )
            };
        }
        OBSERVER.set(Some(observer));

        // The app launches into full screen, which may already have finished.
        if ns_window
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen)
        {
            enter(ns_window, mtm);
        }
    });
}

fn enter(window: &NSWindow, mtm: MainThreadMarker) {
    set_title_bar_hidden(window, true);
    INSTALLED.with_borrow_mut(|installed| {
        if installed.is_none() {
            *installed = install(window, mtm);
        }
    });
}

fn leave(window: &NSWindow) {
    INSTALLED.with_borrow_mut(|installed| {
        if let Some(group) = installed.take() {
            group.removeFromSuperview();
        }
    });
    set_title_bar_hidden(window, false);
}

fn install(window: &NSWindow, mtm: MainThreadMarker) -> Option<Retained<TrafficLights>> {
    let content = window.contentView()?;
    let style = window.styleMask();
    let buttons = [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ]
    .iter()
    .map(|&kind| NSWindow::standardWindowButton_forStyleMask(kind, style, mtm))
    .collect::<Option<Vec<Retained<NSButton>>>>()?;

    let spacing = match (
        window.standardWindowButton(NSWindowButton::CloseButton),
        window.standardWindowButton(NSWindowButton::MiniaturizeButton),
    ) {
        (Some(close), Some(minimize)) => minimize.frame().origin.x - close.frame().origin.x,
        _ => 0.0,
    };
    let spacing = if spacing > 0.0 {
        spacing
    } else {
        DEFAULT_SPACING
    };

    let size = buttons[0].frame().size;
    let top = (BAR_HEIGHT - size.height) / 2.0;
    let flipped = content.isFlipped();
    let y = if flipped {
        top
    } else {
        content.bounds().size.height - top - size.height
    };
    let group = TrafficLights::new(
        mtm,
        NSRect::new(
            NSPoint::new(LEFT, y),
            NSSize::new(spacing * 2.0 + size.width, size.height),
        ),
    );
    // Stay pinned to the top edge as the window resizes.
    group.setAutoresizingMask(if flipped {
        NSAutoresizingMaskOptions::ViewMaxYMargin
    } else {
        NSAutoresizingMaskOptions::ViewMinYMargin
    });
    for (index, button) in buttons.iter().enumerate() {
        button.setFrameOrigin(NSPoint::new(index as f64 * spacing, 0.0));
        group.addSubview(button);
    }
    // A full screen window can't be minimized; AppKit greys this one out too.
    buttons[1].setEnabled(false);

    content.addSubview(&group);
    Some(group)
}

/// Hides or restores the window's own title bar. In full screen it lives in a
/// separate strip window that slides down with the menu bar; hiding it and
/// letting clicks through leaves only the menu bar.
fn set_title_bar_hidden(window: &NSWindow, hidden: bool) {
    let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
        return;
    };
    // close → NSTitlebarView → NSTitlebarContainerView, as tao assumes too.
    // SAFETY: plain view hierarchy reads on the main thread.
    if let Some(container) = unsafe { close.superview().and_then(|view| view.superview()) } {
        container.setHidden(hidden);
    }
    if let Some(strip) = close.window() {
        if !std::ptr::eq(&*strip, window) {
            strip.setAlphaValue(if hidden { 0.0 } else { 1.0 });
            strip.setIgnoresMouseEvents(hidden);
        }
    }
}
