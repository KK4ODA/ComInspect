//! Change notifications on Windows:
//!
//! * `CM_Register_Notification` for device-interface arrival/removal (all
//!   interface classes) and device-instance changes. Callback based, no
//!   window or message loop required.
//! * `RegNotifyChangeKeyValue` on `HKLM\HARDWARE\DEVICEMAP`, which catches
//!   ports created by non-Plug-and-Play virtual-port drivers (they only
//!   appear in the SERIALCOMM map).

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
    CM_NOTIFY_FILTER_FLAG_ALL_DEVICE_INSTANCES, CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES,
    CM_NOTIFY_FILTER_TYPE_DEVICEINSTANCE, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
    CM_Register_Notification, CM_Unregister_Notification, CR_SUCCESS, HCMNOTIFICATION,
};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
    RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::monitor::{ChangeHint, HintSource};

unsafe extern "system" fn on_pnp_event(
    _handle: HCMNOTIFICATION,
    context: *const c_void,
    _action: CM_NOTIFY_ACTION,
    _data: *const CM_NOTIFY_EVENT_DATA,
    _size: u32,
) -> u32 {
    if !context.is_null() {
        // SAFETY: `context` is the `Sender` boxed by `PnpSource`, which
        // outlives the registration (it is unregistered before being freed).
        let tx = unsafe { &*(context as *const Sender<ChangeHint>) };
        let _ = tx.send(ChangeHint::Os("pnp"));
    }
    ERROR_SUCCESS
}

struct PnpSource {
    handles: Vec<HCMNOTIFICATION>,
    context: *mut Sender<ChangeHint>,
}

// SAFETY: the raw pointers are only used to unregister/free on drop; the
// callback context is a `Sender`, which is `Send + Sync`.
unsafe impl Send for PnpSource {}

impl HintSource for PnpSource {
    fn name(&self) -> &'static str {
        "pnp-notifications"
    }
}

impl Drop for PnpSource {
    fn drop(&mut self) {
        for handle in self.handles.drain(..) {
            // SAFETY: handles came from a successful CM_Register_Notification.
            // Unregistering waits for in-flight callbacks to finish.
            unsafe { CM_Unregister_Notification(handle) };
        }
        if !self.context.is_null() {
            // SAFETY: allocated with Box::into_raw in `register_pnp` and no
            // callback can run any more.
            drop(unsafe { Box::from_raw(self.context) });
        }
    }
}

fn register_pnp(tx: Sender<ChangeHint>) -> Option<PnpSource> {
    let context = Box::into_raw(Box::new(tx));
    let mut source = PnpSource {
        handles: Vec::new(),
        context,
    };
    for (filter_type, flags) in [
        (
            CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            CM_NOTIFY_FILTER_FLAG_ALL_INTERFACE_CLASSES,
        ),
        (
            CM_NOTIFY_FILTER_TYPE_DEVICEINSTANCE,
            CM_NOTIFY_FILTER_FLAG_ALL_DEVICE_INSTANCES,
        ),
    ] {
        let filter = CM_NOTIFY_FILTER {
            cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
            Flags: flags,
            FilterType: filter_type,
            ..Default::default()
        };
        let mut handle: HCMNOTIFICATION = ptr::null_mut();
        // SAFETY: filter is initialized, context stays valid until drop.
        let cr = unsafe {
            CM_Register_Notification(
                &filter,
                context as *const c_void,
                Some(on_pnp_event),
                &mut handle,
            )
        };
        if cr == CR_SUCCESS && !handle.is_null() {
            source.handles.push(handle);
        } else {
            log::warn!("CM_Register_Notification failed (CONFIGRET {cr})");
        }
    }
    if source.handles.is_empty() {
        None
    } else {
        Some(source)
    }
}

struct DeviceMapSource {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl HintSource for DeviceMapSource {
    fn name(&self) -> &'static str {
        "serial-device-map"
    }
}

impl Drop for DeviceMapSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn watch_device_map(tx: Sender<ChangeHint>) -> Option<DeviceMapSource> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let thread = thread::Builder::new()
        .name("devicemap-watch".into())
        .spawn(move || {
            let path: Vec<u16> = "HARDWARE\\DEVICEMAP"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut hkey: HKEY = ptr::null_mut();
            // SAFETY: valid pointers.
            let rc = unsafe {
                RegOpenKeyExW(HKEY_LOCAL_MACHINE, path.as_ptr(), 0, KEY_NOTIFY, &mut hkey)
            };
            if rc != ERROR_SUCCESS {
                log::warn!("cannot watch the serial device map (error {rc})");
                return;
            }
            // SAFETY: plain event creation (auto-reset, non-signaled).
            let event: HANDLE = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
            if event.is_null() {
                // SAFETY: hkey was opened above.
                unsafe { RegCloseKey(hkey) };
                return;
            }
            while !stop_flag.load(Ordering::SeqCst) {
                // SAFETY: hkey and event are valid; asynchronous registration
                // lives as long as this thread.
                let rc = unsafe {
                    RegNotifyChangeKeyValue(
                        hkey,
                        1,
                        REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET,
                        event,
                        1,
                    )
                };
                if rc != ERROR_SUCCESS {
                    log::warn!("RegNotifyChangeKeyValue failed (error {rc})");
                    break;
                }
                loop {
                    // SAFETY: event is a valid handle.
                    let wait = unsafe { WaitForSingleObject(event, 500) };
                    if wait == WAIT_OBJECT_0 {
                        if tx.send(ChangeHint::Os("serialcomm")).is_err() {
                            stop_flag.store(true, Ordering::SeqCst);
                        }
                        break;
                    }
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
            // SAFETY: both handles were created above.
            unsafe {
                CloseHandle(event);
                RegCloseKey(hkey);
            }
        })
        .ok()?;
    Some(DeviceMapSource {
        stop,
        thread: Some(thread),
    })
}

pub(crate) fn start_sources(tx: Sender<ChangeHint>) -> Vec<Box<dyn HintSource>> {
    let mut sources: Vec<Box<dyn HintSource>> = Vec::new();
    if let Some(pnp) = register_pnp(tx.clone()) {
        sources.push(Box::new(pnp));
    }
    if let Some(map) = watch_device_map(tx) {
        sources.push(Box::new(map));
    }
    sources
}
