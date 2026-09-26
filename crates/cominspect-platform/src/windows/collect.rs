//! Raw data collection on Windows through documented, supported APIs:
//!
//! * Configuration Manager (`cfgmgr32`): device lists per setup class
//!   (including non-present devices), device properties (`DEVPKEY_*`),
//!   parent chain, status/problem codes, and the devnode hardware key for
//!   `PortName`.
//! * The serial device map `HKLM\HARDWARE\DEVICEMAP\SERIALCOMM` (active ports,
//!   including non-Plug-and-Play virtual ports).
//! * The COM port database (`msports.dll` `ComDB*`), with a read-only
//!   registry fallback when the database cannot be opened.
//!
//! All unsafe code is confined to this file and every call's result is
//! checked; failures degrade to missing properties, never panics.

use std::collections::{BTreeMap, HashMap};
use std::ptr;
use std::time::Instant;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_ALL_DEVICES, CM_GETIDLIST_FILTER_CLASS,
    CM_GETIDLIST_FILTER_ENUMERATOR, CM_Get_DevNode_PropertyW, CM_Get_DevNode_Status,
    CM_Get_Device_ID_List_SizeW, CM_Get_Device_ID_ListW, CM_Get_Device_IDW,
    CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_ListW,
    CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_LOCATE_DEVNODE_PHANTOM,
    CM_Locate_DevNodeW, CM_Open_DevNode_Key, CM_REGISTRY_HARDWARE, CR_BUFFER_SMALL, CR_SUCCESS,
    DN_HAS_PROBLEM, RegDisposition_OpenExisting,
};
use windows_sys::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_Class, DEVPKEY_Device_ClassGuid,
    DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_ContainerId, DEVPKEY_Device_DeviceDesc,
    DEVPKEY_Device_DriverDate, DEVPKEY_Device_DriverInfPath, DEVPKEY_Device_DriverProvider,
    DEVPKEY_Device_DriverVersion, DEVPKEY_Device_EnumeratorName, DEVPKEY_Device_FirstInstallDate,
    DEVPKEY_Device_FriendlyName, DEVPKEY_Device_HardwareIds, DEVPKEY_Device_InstallDate,
    DEVPKEY_Device_InstanceId, DEVPKEY_Device_IsPresent, DEVPKEY_Device_LastArrivalDate,
    DEVPKEY_Device_LastRemovalDate, DEVPKEY_Device_LocationInfo, DEVPKEY_Device_LocationPaths,
    DEVPKEY_Device_Manufacturer, DEVPKEY_Device_Parent, DEVPKEY_Device_Service,
    DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_FILETIME, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_STRING_LIST, DEVPROPTYPE,
};
use windows_sys::Win32::Devices::SerialCommunication::{
    CDB_REPORT_BITS, ComDBClose, ComDBGetCurrentPortUsage, ComDBOpen, HCOMDB,
};
use windows_sys::Win32::Foundation::{DEVPROPKEY, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_READ, REG_BINARY, REG_EXPAND_SZ, REG_SZ,
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW,
};
use windows_sys::core::GUID;

use cominspect_core::model::ScanResult;
use cominspect_core::time::{filetime_to_unix_ms, now_ms};

use super::interpret::{
    RawComDb, RawDevNode, RawPort, RawSerialComm, RawWindowsScan, bluetooth_address, interpret,
};

const PORTS_CLASS: &str = "{4d36e978-e325-11ce-bfc1-08002be10318}";
const MODEM_CLASS: &str = "{4d36e96d-e325-11ce-bfc1-08002be10318}";
const GUID_DEVINTERFACE_COMPORT: GUID = GUID::from_u128(0x86e0d1e0_8089_11d0_9ce4_08003e301f73);
const MAX_DEVICE_ID_LEN: usize = 200;
const MAX_ANCESTORS: usize = 8;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn multi_sz(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

fn u16_units(bytes: &[u8]) -> Vec<u16> {
    let (pairs, _) = bytes.as_chunks::<2>();
    pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect()
}

fn format_guid(b: &[u8]) -> Option<String> {
    if b.len() < 16 {
        return None;
    }
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    Some(format!(
        "{{{d1:08x}-{d2:04x}-{d3:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

// --- device lists --------------------------------------------------------------

/// Device instance IDs matching a filter (`CM_Get_Device_ID_ListW`).
fn device_id_list(filter: &str, flags: u32) -> Vec<String> {
    let filter_w = wide(filter);
    for _ in 0..4 {
        let mut len: u32 = 0;
        // SAFETY: valid out pointer and NUL-terminated filter.
        let cr = unsafe { CM_Get_Device_ID_List_SizeW(&mut len, filter_w.as_ptr(), flags) };
        if cr != CR_SUCCESS || len == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: buffer has `len` elements as required.
        let cr = unsafe { CM_Get_Device_ID_ListW(filter_w.as_ptr(), buf.as_mut_ptr(), len, flags) };
        if cr == CR_BUFFER_SMALL {
            continue; // the list changed between the two calls
        }
        if cr != CR_SUCCESS {
            return Vec::new();
        }
        return multi_sz(&buf);
    }
    Vec::new()
}

/// Instance IDs of all devices (present or not) exposing an interface class.
fn interface_device_ids(class: &GUID) -> Vec<String> {
    let mut interfaces = Vec::new();
    for _ in 0..4 {
        let mut len: u32 = 0;
        // SAFETY: valid pointers; a NULL device ID means "all devices".
        let cr = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                class,
                ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_ALL_DEVICES,
            )
        };
        if cr != CR_SUCCESS || len == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: buffer has `len` elements.
        let cr = unsafe {
            CM_Get_Device_Interface_ListW(
                class,
                ptr::null(),
                buf.as_mut_ptr(),
                len,
                CM_GET_DEVICE_INTERFACE_LIST_ALL_DEVICES,
            )
        };
        if cr == CR_BUFFER_SMALL {
            continue;
        }
        if cr == CR_SUCCESS {
            interfaces = multi_sz(&buf);
        }
        break;
    }
    interfaces
        .iter()
        .filter_map(|path| {
            let path_w = wide(path);
            let mut ty: DEVPROPTYPE = 0;
            let mut size: u32 = 0;
            // SAFETY: size query with NULL buffer.
            let cr = unsafe {
                CM_Get_Device_Interface_PropertyW(
                    path_w.as_ptr(),
                    &DEVPKEY_Device_InstanceId,
                    &mut ty,
                    ptr::null_mut(),
                    &mut size,
                    0,
                )
            };
            if cr != CR_BUFFER_SMALL || size == 0 {
                return None;
            }
            let mut bytes = vec![0u8; size as usize];
            // SAFETY: buffer of `size` bytes.
            let cr = unsafe {
                CM_Get_Device_Interface_PropertyW(
                    path_w.as_ptr(),
                    &DEVPKEY_Device_InstanceId,
                    &mut ty,
                    bytes.as_mut_ptr(),
                    &mut size,
                    0,
                )
            };
            (cr == CR_SUCCESS && ty == DEVPROP_TYPE_STRING)
                .then(|| from_wide(&u16_units(&bytes[..size as usize])))
        })
        .collect()
}

// --- devnode access ----------------------------------------------------------

fn locate(instance_id: &str) -> Option<u32> {
    let id = wide(instance_id);
    let mut devinst: u32 = 0;
    // SAFETY: valid out pointer and NUL-terminated ID.
    let cr = unsafe { CM_Locate_DevNodeW(&mut devinst, id.as_ptr(), CM_LOCATE_DEVNODE_PHANTOM) };
    (cr == CR_SUCCESS).then_some(devinst)
}

fn device_id(devinst: u32) -> Option<String> {
    let mut buf = vec![0u16; MAX_DEVICE_ID_LEN + 1];
    // SAFETY: buffer length passed matches the allocation.
    let cr = unsafe { CM_Get_Device_IDW(devinst, buf.as_mut_ptr(), buf.len() as u32, 0) };
    (cr == CR_SUCCESS).then(|| from_wide(&buf))
}

fn parent(devinst: u32) -> Option<u32> {
    let mut parent: u32 = 0;
    // SAFETY: valid out pointer.
    let cr = unsafe { CM_Get_Parent(&mut parent, devinst, 0) };
    (cr == CR_SUCCESS).then_some(parent)
}

fn prop_raw(devinst: u32, key: &DEVPROPKEY) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut ty: DEVPROPTYPE = 0;
    let mut size: u32 = 0;
    // SAFETY: size query with NULL buffer.
    let cr =
        unsafe { CM_Get_DevNode_PropertyW(devinst, key, &mut ty, ptr::null_mut(), &mut size, 0) };
    if cr != CR_BUFFER_SMALL || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buffer of `size` bytes.
    let cr =
        unsafe { CM_Get_DevNode_PropertyW(devinst, key, &mut ty, buf.as_mut_ptr(), &mut size, 0) };
    if cr != CR_SUCCESS {
        return None;
    }
    buf.truncate(size as usize);
    Some((ty, buf))
}

fn prop_string(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    let (ty, buf) = prop_raw(devinst, key)?;
    (ty == DEVPROP_TYPE_STRING)
        .then(|| from_wide(&u16_units(&buf)))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn prop_strings(devinst: u32, key: &DEVPROPKEY) -> Vec<String> {
    match prop_raw(devinst, key) {
        Some((ty, buf)) if ty == DEVPROP_TYPE_STRING_LIST => multi_sz(&u16_units(&buf)),
        Some((ty, buf)) if ty == DEVPROP_TYPE_STRING => vec![from_wide(&u16_units(&buf))],
        _ => Vec::new(),
    }
}

fn prop_bool(devinst: u32, key: &DEVPROPKEY) -> Option<bool> {
    let (ty, buf) = prop_raw(devinst, key)?;
    (ty == DEVPROP_TYPE_BOOLEAN && !buf.is_empty()).then(|| buf[0] != 0)
}

fn prop_filetime(devinst: u32, key: &DEVPROPKEY) -> Option<i64> {
    let (ty, buf) = prop_raw(devinst, key)?;
    if ty != DEVPROP_TYPE_FILETIME || buf.len() < 8 {
        return None;
    }
    let raw = u64::from_le_bytes(buf[..8].try_into().ok()?);
    filetime_to_unix_ms(raw)
}

fn prop_guid(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    let (ty, buf) = prop_raw(devinst, key)?;
    (ty == DEVPROP_TYPE_GUID)
        .then(|| format_guid(&buf))
        .flatten()
}

/// `(present, problem code)` from `CM_Get_DevNode_Status`.
fn status(devinst: u32) -> (bool, Option<u32>) {
    let mut status: u32 = 0;
    let mut problem: u32 = 0;
    // SAFETY: valid out pointers.
    let cr = unsafe { CM_Get_DevNode_Status(&mut status, &mut problem, devinst, 0) };
    if cr != CR_SUCCESS {
        return (false, None);
    }
    let has_problem = status & DN_HAS_PROBLEM != 0;
    (true, has_problem.then_some(problem))
}

fn reg_query_string(hkey: HKEY, name: &str) -> Option<String> {
    let name_w = wide(name);
    let mut ty: u32 = 0;
    let mut size: u32 = 0;
    // SAFETY: size query with NULL data pointer.
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name_w.as_ptr(),
            ptr::null(),
            &mut ty,
            ptr::null_mut(),
            &mut size,
        )
    };
    if rc != ERROR_SUCCESS || (ty != REG_SZ && ty != REG_EXPAND_SZ) || size == 0 || size > 4096 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: buffer of `size` bytes.
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name_w.as_ptr(),
            ptr::null(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut size,
        )
    };
    if rc != ERROR_SUCCESS {
        return None;
    }
    let value = from_wide(&u16_units(&buf[..size as usize]));
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// `PortName` from the devnode's hardware key ("Device Parameters").
fn port_name(devinst: u32) -> Option<String> {
    let mut hkey: HKEY = ptr::null_mut();
    // SAFETY: valid out pointer; read-only access.
    let cr = unsafe {
        CM_Open_DevNode_Key(
            devinst,
            KEY_QUERY_VALUE,
            0,
            RegDisposition_OpenExisting,
            &mut hkey,
            CM_REGISTRY_HARDWARE,
        )
    };
    if cr != CR_SUCCESS || hkey.is_null() {
        return None;
    }
    let value = reg_query_string(hkey, "PortName");
    // SAFETY: hkey was opened above.
    unsafe { RegCloseKey(hkey) };
    value
}

fn read_node(devinst: u32, instance_id: &str, full: bool) -> RawDevNode {
    let (status_ok, problem) = status(devinst);
    let present = prop_bool(devinst, &DEVPKEY_Device_IsPresent).unwrap_or(status_ok);
    let mut node = RawDevNode {
        instance_id: instance_id.to_string(),
        present,
        problem: if present { problem } else { None },
        friendly_name: prop_string(devinst, &DEVPKEY_Device_FriendlyName),
        device_desc: prop_string(devinst, &DEVPKEY_Device_DeviceDesc),
        bus_reported_desc: prop_string(devinst, &DEVPKEY_Device_BusReportedDeviceDesc),
        hardware_ids: prop_strings(devinst, &DEVPKEY_Device_HardwareIds),
        enumerator: prop_string(devinst, &DEVPKEY_Device_EnumeratorName),
        location_info: prop_string(devinst, &DEVPKEY_Device_LocationInfo),
        location_paths: prop_strings(devinst, &DEVPKEY_Device_LocationPaths),
        parent: prop_string(devinst, &DEVPKEY_Device_Parent),
        ..Default::default()
    };
    if full {
        node.manufacturer = prop_string(devinst, &DEVPKEY_Device_Manufacturer);
        node.compatible_ids = prop_strings(devinst, &DEVPKEY_Device_CompatibleIds);
        node.class = prop_string(devinst, &DEVPKEY_Device_Class);
        node.class_guid = prop_guid(devinst, &DEVPKEY_Device_ClassGuid);
        node.service = prop_string(devinst, &DEVPKEY_Device_Service);
        node.driver_provider = prop_string(devinst, &DEVPKEY_Device_DriverProvider);
        node.driver_version = prop_string(devinst, &DEVPKEY_Device_DriverVersion);
        node.driver_date = prop_filetime(devinst, &DEVPKEY_Device_DriverDate);
        node.driver_inf = prop_string(devinst, &DEVPKEY_Device_DriverInfPath);
        node.container_id = prop_guid(devinst, &DEVPKEY_Device_ContainerId);
        node.install_date = prop_filetime(devinst, &DEVPKEY_Device_InstallDate);
        node.first_install_date = prop_filetime(devinst, &DEVPKEY_Device_FirstInstallDate);
        node.last_arrival_date = prop_filetime(devinst, &DEVPKEY_Device_LastArrivalDate);
        node.last_removal_date = prop_filetime(devinst, &DEVPKEY_Device_LastRemovalDate);
        node.port_name = port_name(devinst);
    }
    node
}

fn collect_port(instance_id: &str) -> Option<RawPort> {
    let devinst = locate(instance_id)?;
    let node = read_node(devinst, instance_id, true);

    let mut ancestors = Vec::new();
    let mut current = devinst;
    let mut next_id = node.parent.clone();
    for _ in 0..MAX_ANCESTORS {
        // Prefer the live tree; fall back to the persisted Parent property
        // (useful for non-present devices).
        let (parent_inst, parent_id) = match parent(current).and_then(|p| Some((p, device_id(p)?)))
        {
            Some(pair) => pair,
            None => match next_id.take().and_then(|id| Some((locate(&id)?, id))) {
                Some(pair) => pair,
                None => break,
            },
        };
        if parent_id.to_ascii_uppercase().starts_with("HTREE\\ROOT") {
            break;
        }
        let ancestor = read_node(parent_inst, &parent_id, false);
        next_id.clone_from(&ancestor.parent);
        ancestors.push(ancestor);
        current = parent_inst;
    }
    Some(RawPort { node, ancestors })
}

// --- registry maps -----------------------------------------------------------

fn open_hklm(path: &str, access: u32) -> Option<HKEY> {
    let path_w = wide(path);
    let mut hkey: HKEY = ptr::null_mut();
    // SAFETY: valid pointers.
    let rc = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path_w.as_ptr(), 0, access, &mut hkey) };
    (rc == ERROR_SUCCESS && !hkey.is_null()).then_some(hkey)
}

/// Values of `HKLM\HARDWARE\DEVICEMAP\SERIALCOMM`.
fn read_serialcomm() -> Vec<RawSerialComm> {
    let Some(hkey) = open_hklm("HARDWARE\\DEVICEMAP\\SERIALCOMM", KEY_READ) else {
        return Vec::new(); // no active serial ports
    };
    let mut out = Vec::new();
    for index in 0..1024u32 {
        let mut name = vec![0u16; 512];
        let mut name_len = name.len() as u32;
        let mut data = vec![0u8; 512];
        let mut data_len = data.len() as u32;
        let mut ty: u32 = 0;
        // SAFETY: all buffers and their lengths are valid.
        let rc = unsafe {
            RegEnumValueW(
                hkey,
                index,
                name.as_mut_ptr(),
                &mut name_len,
                ptr::null(),
                &mut ty,
                data.as_mut_ptr(),
                &mut data_len,
            )
        };
        if rc != ERROR_SUCCESS {
            break; // ERROR_NO_MORE_ITEMS or an oversized value
        }
        if ty != REG_SZ {
            continue;
        }
        let kernel_name = String::from_utf16_lossy(&name[..name_len as usize]);
        let port = from_wide(&u16_units(&data[..data_len as usize]));
        if !port.trim().is_empty() {
            out.push(RawSerialComm {
                kernel_name,
                port_name: port.trim().to_string(),
            });
        }
    }
    // SAFETY: hkey was opened above.
    unsafe { RegCloseKey(hkey) };
    out
}

/// Reserved COM numbers from the COM port database.
fn read_comdb(warnings: &mut Vec<String>) -> Option<RawComDb> {
    let mut handle: HCOMDB = ptr::null_mut();
    // SAFETY: valid out pointer.
    let rc = unsafe { ComDBOpen(&mut handle) };
    if rc == ERROR_SUCCESS as i32 && !handle.is_null() && handle as isize != -1 {
        let mut max: u32 = 0;
        // SAFETY: size query with NULL buffer.
        let rc = unsafe {
            ComDBGetCurrentPortUsage(handle, ptr::null_mut(), 0, CDB_REPORT_BITS, &mut max)
        };
        let result = if rc == ERROR_SUCCESS as i32 && max > 0 && max <= 4096 {
            let mut bitmap = vec![0u8; max.div_ceil(8) as usize];
            let mut reported: u32 = 0;
            // SAFETY: buffer length matches the allocation.
            let rc = unsafe {
                ComDBGetCurrentPortUsage(
                    handle,
                    bitmap.as_mut_ptr(),
                    bitmap.len() as u32,
                    CDB_REPORT_BITS,
                    &mut reported,
                )
            };
            (rc == ERROR_SUCCESS as i32).then(|| RawComDb {
                bitmap,
                database_size: reported.max(1).min(max),
                source: "COM port database (ComDB API)".into(),
            })
        } else {
            None
        };
        // SAFETY: handle was opened above.
        unsafe { ComDBClose(handle) };
        if result.is_some() {
            return result;
        }
    }

    // Read-only fallback: the database's registry value (documented bitmap).
    let hkey = open_hklm(
        "SYSTEM\\CurrentControlSet\\Control\\COM Name Arbiter",
        KEY_QUERY_VALUE,
    )?;
    let name = wide("ComDB");
    let mut ty: u32 = 0;
    let mut size: u32 = 0;
    // SAFETY: size query.
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            name.as_ptr(),
            ptr::null(),
            &mut ty,
            ptr::null_mut(),
            &mut size,
        )
    };
    let mut result = None;
    if rc == ERROR_SUCCESS && ty == REG_BINARY && size > 0 && size <= 512 {
        let mut bitmap = vec![0u8; size as usize];
        // SAFETY: buffer of `size` bytes.
        let rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                ptr::null(),
                &mut ty,
                bitmap.as_mut_ptr(),
                &mut size,
            )
        };
        if rc == ERROR_SUCCESS {
            bitmap.truncate(size as usize);
            result = Some(RawComDb {
                database_size: (bitmap.len() * 8) as u32,
                bitmap,
                source: "COM port database (read-only registry fallback)".into(),
            });
        }
    }
    // SAFETY: hkey was opened above.
    unsafe { RegCloseKey(hkey) };
    if result.is_none() {
        warnings.push("reserved COM numbers could not be read".into());
    }
    result
}

/// Bluetooth address → paired device name.
fn bluetooth_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    for id in device_id_list("BTHENUM", CM_GETIDLIST_FILTER_ENUMERATOR) {
        if !id.to_ascii_uppercase().starts_with("BTHENUM\\DEV_") {
            continue;
        }
        let Some((address, _)) = bluetooth_address(&id) else {
            continue;
        };
        let Some(devinst) = locate(&id) else { continue };
        let name = prop_string(devinst, &DEVPKEY_Device_FriendlyName)
            .or_else(|| prop_string(devinst, &DEVPKEY_Device_BusReportedDeviceDesc))
            .or_else(|| prop_string(devinst, &DEVPKEY_Device_DeviceDesc));
        if let Some(name) = name {
            names.insert(address, name);
        }
    }
    names
}

/// Collects raw data for every serial-capable device node.
pub fn collect() -> RawWindowsScan {
    let mut warnings = Vec::new();
    // Upper-case ID → original spelling, so each device is read once.
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    for class in [PORTS_CLASS, MODEM_CLASS] {
        for id in device_id_list(class, CM_GETIDLIST_FILTER_CLASS) {
            ids.entry(id.to_ascii_uppercase()).or_insert(id);
        }
    }
    for id in interface_device_ids(&GUID_DEVINTERFACE_COMPORT) {
        ids.entry(id.to_ascii_uppercase()).or_insert(id);
    }
    if ids.is_empty() {
        log::debug!("no devices in the Ports/Modem classes");
    }

    let ports: Vec<RawPort> = ids.values().filter_map(|id| collect_port(id)).collect();
    let serialcomm = read_serialcomm();
    let comdb = read_comdb(&mut warnings);
    RawWindowsScan {
        ports,
        serialcomm,
        bluetooth_names: bluetooth_names(),
        comdb,
        warnings,
    }
}

pub fn discover() -> ScanResult {
    let started = Instant::now();
    let raw = collect();
    interpret(raw, now_ms(), started.elapsed().as_millis() as u64)
}
