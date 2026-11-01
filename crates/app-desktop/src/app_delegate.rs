//! What macOS asks the application delegate and winit's delegate does not answer.
//!
//! - `application:openURLs:`: opening a project from the Finder (double-click, Open With,
//!   dropping on the Dock icon). Without it macOS said "cannot open files in the DAW 388 Project
//!   format".
//! - `applicationShouldTerminate:`: logging out, restarting or Quit in the Dock menu. Without it
//!   the app ended without the "save changes?" question. Now it answers "later", the UI asks, and
//!   the answer goes back to macOS (`reply_to_terminate`).
//!
//! It also tells where the mouse is while files are dragged over the window (`pointer_in_window`),
//! which winit does not pass on.
//!
//! Once AppKit announces it will finish launching, the delegate exists and the methods are added
//! to its class; AppKit asks for them only when they are needed, so this is in time for the files
//! that start the app as well.

#![allow(unsafe_code)]

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{ffi, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSURL, ns_string};
use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Files macOS asked the app to open, waiting for the next frame.
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// macOS asked to quit and waits for the answer.
static TERMINATE_ASKED: AtomicBool = AtomicBool::new(false);
/// Wakes the UI when macOS asks something while the app runs.
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

/// `NSTerminateLater` of `NSApplicationTerminateReply`.
const TERMINATE_LATER: usize = 2;

type OpenUrls = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>);
type ShouldTerminate = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) -> usize;

fn wake() {
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

extern "C-unwind" fn open_urls(_delegate: &AnyObject, _cmd: Sel, _app: &AnyObject, urls: &NSArray<NSURL>) {
    let paths: Vec<PathBuf> = urls.iter().filter_map(|url| url.path()).map(|path| PathBuf::from(path.to_string())).collect();
    log::info!("opened from the Finder: {paths:?}");
    if let Ok(mut opened) = OPENED.lock() {
        opened.extend(paths);
    }
    wake();
}

extern "C-unwind" fn should_terminate(_delegate: &AnyObject, _cmd: Sel, _app: &AnyObject) -> usize {
    log::info!("macOS asks to quit");
    TERMINATE_ASKED.store(true, Ordering::Relaxed);
    wake();
    TERMINATE_LATER
}

/// Add `imp` as `name` to the class, unless the class already has that method.
///
/// SAFETY: `imp` must be a function with the signature `types` describes.
unsafe fn add_method(class: &AnyClass, name: Sel, imp: Imp, types: &CStr) {
    if class.instance_method(name).is_none() {
        // SAFETY: the caller vouches for `imp` and `types`.
        unsafe { ffi::class_addMethod((class as *const AnyClass).cast_mut(), name, imp, types.as_ptr()) };
    }
}

/// Teach winit's application delegate the missing methods as soon as it is in place.
/// Call before the event loop starts.
pub fn install() {
    let block = block2::RcBlock::new(|_: std::ptr::NonNull<NSNotification>| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = app.delegate() else { return };
        let delegate: Retained<AnyObject> = delegate.into();
        let class: &AnyClass = delegate.class();
        // SAFETY: each function has the signature of its method: `application:openURLs:` returns
        // nothing and takes the app and an array ("v@:@@"), `applicationShouldTerminate:` takes
        // the app and returns an NSApplicationTerminateReply, an NSUInteger ("Q@:@"). The class
        // is the delegate's own, so the methods only ever receive that delegate.
        unsafe {
            let imp = std::mem::transmute::<OpenUrls, Imp>(open_urls);
            add_method(class, sel!(application:openURLs:), imp, c"v@:@@");
            let imp = std::mem::transmute::<ShouldTerminate, Imp>(should_terminate);
            add_method(class, sel!(applicationShouldTerminate:), imp, c"Q@:@");
        }
    });
    let center = NSNotificationCenter::defaultCenter();
    // The observer stays for the life of the app.
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(ns_string!("NSApplicationWillFinishLaunchingNotification")),
            None,
            None,
            &block,
        )
    };
    std::mem::forget(observer);
}

/// Remember the UI so a request arriving later wakes it.
pub fn set_context(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

/// The files macOS asked to open since the last call.
pub fn take_opened() -> Vec<PathBuf> {
    OPENED.lock().map(|mut opened| std::mem::take(&mut *opened)).unwrap_or_default()
}

/// Whether macOS asked to quit since the last call.
pub fn take_terminate_request() -> bool {
    TERMINATE_ASKED.swap(false, Ordering::Relaxed)
}

/// Tell macOS whether the app may quit after a terminate request. With `true` AppKit ends the
/// app (and goes on with logging out or restarting); with `false` it cancels.
pub fn reply_to_terminate(quit: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    log::info!("quit: {}", if quit { "yes" } else { "cancelled" });
    NSApplication::sharedApplication(mtm).replyToApplicationShouldTerminate(quit);
}

/// Where the mouse is in the app's window, in points from the top left of its content.
pub fn pointer_in_window() -> Option<[f32; 2]> {
    let mtm = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(mtm);
    let window = app.mainWindow().or_else(|| app.keyWindow()).or_else(|| app.windows().firstObject())?;
    let view = window.contentView()?;
    let point = view.convertPoint_fromView(window.mouseLocationOutsideOfEventStream(), None);
    let y = if view.isFlipped() { point.y } else { view.frame().size.height - point.y };
    Some([point.x as f32, y as f32])
}
