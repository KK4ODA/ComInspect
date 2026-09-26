//! IOKit data collection (macOS only).

use std::ffi::{CStr, c_char};
use std::time::Instant;

use core_foundation::base::{CFType, TCFType};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFTypeRef, kCFAllocatorDefault};
use io_kit_sys::keys::kIOServicePlane;
use io_kit_sys::serial::keys::kIOSerialBSDServiceValue;
use io_kit_sys::types::{io_iterator_t, io_object_t, io_registry_entry_t};
use io_kit_sys::{
    IOIteratorNext, IOObjectGetClass, IOObjectRelease, IORegistryEntryCreateCFProperty,
    IORegistryEntryGetParentEntry, IORegistryEntryGetPath, IORegistryEntrySearchCFProperty,
    IOServiceGetMatchingServices, IOServiceMatching, kIORegistryIterateParents,
    kIORegistryIterateRecursively,
};

use cominspect_core::model::ScanResult;
use cominspect_core::time::now_ms;

use super::interpret::{RawMacService, interpret};

const MAX_CHAIN: usize = 12;

fn cf_to_string(value: CFTypeRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: IOKit "Create"/"Search" functions return +1 references.
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value.downcast::<CFString>().map(|s| s.to_string())
}

fn cf_to_i64(value: CFTypeRef) -> Option<i64> {
    if value.is_null() {
        return None;
    }
    // SAFETY: +1 reference from IOKit.
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value.downcast::<CFNumber>().and_then(|n| n.to_i64())
}

fn own_property(entry: io_registry_entry_t, key: &str) -> CFTypeRef {
    let key = CFString::new(key);
    // SAFETY: valid entry and key; the result is owned by the caller.
    unsafe {
        IORegistryEntryCreateCFProperty(entry, key.as_concrete_TypeRef(), kCFAllocatorDefault, 0)
    }
}

fn ancestor_property(entry: io_registry_entry_t, key: &str) -> CFTypeRef {
    let key = CFString::new(key);
    // SAFETY: valid entry, plane and key; the result is owned by the caller.
    unsafe {
        IORegistryEntrySearchCFProperty(
            entry,
            kIOServicePlane,
            key.as_concrete_TypeRef(),
            kCFAllocatorDefault,
            kIORegistryIterateRecursively | kIORegistryIterateParents,
        )
    }
}

fn class_name(entry: io_object_t) -> Option<String> {
    let mut buf = [0 as c_char; 128];
    // SAFETY: io_name_t is a 128-byte buffer.
    let kr = unsafe { IOObjectGetClass(entry, buf.as_mut_ptr()) };
    if kr != 0 {
        return None;
    }
    // SAFETY: IOKit wrote a NUL-terminated string.
    Some(
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn registry_path(entry: io_registry_entry_t) -> Option<String> {
    let mut buf = [0 as c_char; 512];
    // SAFETY: io_string_t is a 512-byte buffer.
    let kr = unsafe { IORegistryEntryGetPath(entry, kIOServicePlane, buf.as_mut_ptr()) };
    if kr != 0 {
        return None;
    }
    // SAFETY: NUL-terminated output.
    Some(
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn class_chain(service: io_registry_entry_t) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = service;
    let mut owned: Vec<io_object_t> = Vec::new();
    for _ in 0..MAX_CHAIN {
        let mut parent: io_registry_entry_t = 0;
        // SAFETY: valid entry and plane; `parent` is an out parameter.
        let kr = unsafe { IORegistryEntryGetParentEntry(current, kIOServicePlane, &mut parent) };
        if kr != 0 || parent == 0 {
            break;
        }
        owned.push(parent);
        if let Some(name) = class_name(parent) {
            chain.push(name);
        }
        current = parent;
    }
    for entry in owned {
        // SAFETY: each parent reference was returned with +1.
        unsafe { IOObjectRelease(entry) };
    }
    chain
}

fn read_service(service: io_registry_entry_t) -> RawMacService {
    let number = |key: &str| cf_to_i64(ancestor_property(service, key));
    RawMacService {
        callout_device: cf_to_string(own_property(service, "IOCalloutDevice")),
        dialin_device: cf_to_string(own_property(service, "IODialinDevice")),
        tty_base_name: cf_to_string(own_property(service, "IOTTYBaseName")),
        tty_suffix: cf_to_string(own_property(service, "IOTTYSuffix")),
        tty_device: cf_to_string(own_property(service, "IOTTYDevice")),
        registry_path: registry_path(service),
        class_chain: class_chain(service),
        vid: number("idVendor").and_then(|v| u16::try_from(v).ok()),
        pid: number("idProduct").and_then(|v| u16::try_from(v).ok()),
        usb_serial: cf_to_string(ancestor_property(service, "USB Serial Number"))
            .or_else(|| cf_to_string(ancestor_property(service, "kUSBSerialNumberString"))),
        usb_vendor_name: cf_to_string(ancestor_property(service, "USB Vendor Name"))
            .or_else(|| cf_to_string(ancestor_property(service, "kUSBVendorString"))),
        usb_product_name: cf_to_string(ancestor_property(service, "USB Product Name"))
            .or_else(|| cf_to_string(ancestor_property(service, "kUSBProductString"))),
        location_id: number("locationID").and_then(|v| u32::try_from(v).ok()),
        interface_number: number("bInterfaceNumber").and_then(|v| u8::try_from(v).ok()),
        bcd_device: number("bcdDevice").and_then(|v| u16::try_from(v).ok()),
    }
}

pub fn collect() -> (Vec<RawMacService>, Vec<String>) {
    let mut warnings = Vec::new();
    // SAFETY: the matching dictionary is consumed by
    // IOServiceGetMatchingServices; MACH_PORT_NULL selects the default port.
    let matching = unsafe { IOServiceMatching(kIOSerialBSDServiceValue) };
    if matching.is_null() {
        warnings.push("IOServiceMatching failed".into());
        return (Vec::new(), warnings);
    }
    let mut iterator: io_iterator_t = 0;
    // SAFETY: valid dictionary and out pointer.
    let kr = unsafe { IOServiceGetMatchingServices(0, matching as _, &mut iterator) };
    if kr != 0 {
        warnings.push(format!("IOServiceGetMatchingServices failed ({kr})"));
        return (Vec::new(), warnings);
    }
    let mut services = Vec::new();
    loop {
        // SAFETY: iterator is valid until released below.
        let service = unsafe { IOIteratorNext(iterator) };
        if service == 0 {
            break;
        }
        services.push(read_service(service));
        // SAFETY: IOIteratorNext returns +1 references.
        unsafe { IOObjectRelease(service) };
    }
    // SAFETY: iterator was returned by IOServiceGetMatchingServices.
    unsafe { IOObjectRelease(iterator) };
    (services, warnings)
}

pub fn discover() -> ScanResult {
    let started = Instant::now();
    let (services, warnings) = collect();
    interpret(
        services,
        now_ms(),
        started.elapsed().as_millis() as u64,
        warnings,
    )
}
