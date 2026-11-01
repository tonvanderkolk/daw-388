//! Opening a project from the Finder (double-click, Open With, dropping on the Dock icon).
//!
//! macOS hands those files to the application delegate as `application:openURLs:`. winit's
//! delegate does not implement it, so macOS answered with "cannot open files in the DAW 388
//! Project format". Once AppKit announces it will finish launching, the delegate exists and the
//! method is added to its class; AppKit asks for it only when a file arrives, so this is in time
//! for the files that start the app as well as for those that come while it runs.

#![allow(unsafe_code)]

use eframe::egui;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{ffi, sel};
use objc2_app_kit::NSApplication;
use objc2_foundation::{MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSURL, ns_string};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Files macOS asked the app to open, waiting for the next frame.
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Wakes the UI when a file arrives while the app runs.
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

type OpenUrls = extern "C-unwind" fn(&AnyObject, Sel, &AnyObject, &NSArray<NSURL>);

extern "C-unwind" fn open_urls(_delegate: &AnyObject, _cmd: Sel, _app: &AnyObject, urls: &NSArray<NSURL>) {
    let paths: Vec<PathBuf> = urls.iter().filter_map(|url| url.path()).map(|path| PathBuf::from(path.to_string())).collect();
    log::info!("opened from the Finder: {paths:?}");
    if let Ok(mut opened) = OPENED.lock() {
        opened.extend(paths);
    }
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

/// Teach winit's application delegate `application:openURLs:` as soon as it is in place.
/// Call before the event loop starts.
pub fn install() {
    let block = block2::RcBlock::new(|_: std::ptr::NonNull<NSNotification>| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let app = NSApplication::sharedApplication(mtm);
        let Some(delegate) = app.delegate() else { return };
        let delegate: Retained<AnyObject> = delegate.into();
        let class: &AnyClass = delegate.class();
        if class.instance_method(sel!(application:openURLs:)).is_some() {
            return;
        }
        // SAFETY: `open_urls` has the signature of `application:openURLs:` ("v@:@@"), and the
        // class is the delegate's own, so the method only ever receives that delegate.
        unsafe {
            let imp = std::mem::transmute::<OpenUrls, Imp>(open_urls);
            ffi::class_addMethod((class as *const AnyClass).cast_mut(), sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
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

/// Remember the UI so a file arriving later wakes it.
pub fn set_context(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

/// The files macOS asked to open since the last call.
pub fn take() -> Vec<PathBuf> {
    OPENED.lock().map(|mut opened| std::mem::take(&mut *opened)).unwrap_or_default()
}
