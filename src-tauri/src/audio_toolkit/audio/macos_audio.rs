//! Minimal CoreAudio FFI to query and set the macOS default output device.
//!
//! Used to auto-switch system output to a higher-quality alternative (e.g.
//! "INZONE Buds - Game") when the user records via a sibling Bluetooth input
//! device that would otherwise force the headset into a low-quality SCO/HFP
//! profile. Only the calls we actually need are bound here — bringing in
//! `coreaudio-rs` as a direct dep would pull a much larger surface.

#![cfg(target_os = "macos")]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use std::ptr;

type OSStatus = i32;
type AudioObjectID = u32;
type AudioObjectPropertySelector = u32;
type AudioObjectPropertyScope = u32;
type AudioObjectPropertyElement = u32;

#[repr(C)]
#[derive(Copy, Clone)]
struct AudioObjectPropertyAddress {
    mSelector: AudioObjectPropertySelector,
    mScope: AudioObjectPropertyScope,
    mElement: AudioObjectPropertyElement,
}

const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectID = 1;
const K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL: AudioObjectPropertyScope = u32::from_be_bytes(*b"glob");
const K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN: AudioObjectPropertyElement = 0;

const K_AUDIO_HARDWARE_PROPERTY_DEVICES: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"dev#");
const K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"dOut");
const K_AUDIO_DEVICE_PROPERTY_STREAMS: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"stm#");
const K_AUDIO_OBJECT_PROPERTY_NAME: AudioObjectPropertySelector =
    u32::from_be_bytes(*b"lnam");
const K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT: AudioObjectPropertyScope = u32::from_be_bytes(*b"outp");

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> OSStatus;

    fn AudioObjectGetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;

    fn AudioObjectSetPropertyData(
        in_object_id: AudioObjectID,
        in_address: *const AudioObjectPropertyAddress,
        in_qualifier_data_size: u32,
        in_qualifier_data: *const c_void,
        in_data_size: u32,
        in_data: *const c_void,
    ) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetLength(theString: *const c_void) -> isize;
    fn CFStringGetCString(
        theString: *const c_void,
        buffer: *mut u8,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFRelease(cf: *const c_void);
}
const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

fn list_all_device_ids() -> Vec<AudioObjectID> {
    unsafe {
        let address = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_PROPERTY_DEVICES,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut data_size: u32 = 0;
        let status = AudioObjectGetPropertyDataSize(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            ptr::null(),
            &mut data_size,
        );
        if status != 0 || data_size == 0 {
            return Vec::new();
        }

        let count = data_size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids: Vec<AudioObjectID> = vec![0; count];
        let mut io_size = data_size;
        let status = AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            ptr::null(),
            &mut io_size,
            ids.as_mut_ptr() as *mut c_void,
        );
        if status != 0 {
            return Vec::new();
        }
        ids
    }
}

fn device_name(device_id: AudioObjectID) -> Option<String> {
    unsafe {
        let address = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_OBJECT_PROPERTY_NAME,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut cfstr: *const c_void = ptr::null();
        let mut io_size: u32 = std::mem::size_of::<*const c_void>() as u32;
        let status = AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut io_size,
            &mut cfstr as *mut _ as *mut c_void,
        );
        if status != 0 || cfstr.is_null() {
            return None;
        }

        let len = CFStringGetLength(cfstr);
        // Worst-case UTF-8: 4 bytes per UTF-16 unit, plus NUL.
        let buf_size = (len * 4) + 1;
        let mut buf = vec![0u8; buf_size as usize];
        let ok = CFStringGetCString(cfstr, buf.as_mut_ptr(), buf_size, K_CF_STRING_ENCODING_UTF8);
        CFRelease(cfstr);
        if !ok {
            return None;
        }
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..nul].to_vec()).ok()
    }
}

fn device_has_output_streams(device_id: AudioObjectID) -> bool {
    unsafe {
        let address = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_DEVICE_PROPERTY_STREAMS,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_OUTPUT,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };
        let mut data_size: u32 = 0;
        let status = AudioObjectGetPropertyDataSize(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut data_size,
        );
        status == 0 && data_size > 0
    }
}

pub fn get_default_output_device_name() -> Option<String> {
    unsafe {
        let address = AudioObjectPropertyAddress {
            mSelector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
            mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
            mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
        };

        let mut device_id: AudioObjectID = 0;
        let mut io_size: u32 = std::mem::size_of::<AudioObjectID>() as u32;
        let status = AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            ptr::null(),
            &mut io_size,
            &mut device_id as *mut _ as *mut c_void,
        );
        if status != 0 || device_id == 0 {
            return None;
        }
        device_name(device_id)
    }
}

pub fn set_default_output_device_by_name(target_name: &str) -> Result<(), String> {
    for id in list_all_device_ids() {
        if !device_has_output_streams(id) {
            continue;
        }
        let Some(name) = device_name(id) else { continue };
        if name == target_name {
            unsafe {
                let address = AudioObjectPropertyAddress {
                    mSelector: K_AUDIO_HARDWARE_PROPERTY_DEFAULT_OUTPUT_DEVICE,
                    mScope: K_AUDIO_OBJECT_PROPERTY_SCOPE_GLOBAL,
                    mElement: K_AUDIO_OBJECT_PROPERTY_ELEMENT_MAIN,
                };
                let id_ref = id;
                let status = AudioObjectSetPropertyData(
                    K_AUDIO_OBJECT_SYSTEM_OBJECT,
                    &address,
                    0,
                    ptr::null(),
                    std::mem::size_of::<AudioObjectID>() as u32,
                    &id_ref as *const _ as *const c_void,
                );
                if status == 0 {
                    return Ok(());
                } else {
                    return Err(format!("AudioObjectSetPropertyData failed: {status}"));
                }
            }
        }
    }
    Err(format!("output device not found: {target_name}"))
}

pub fn list_output_device_names() -> Vec<String> {
    list_all_device_ids()
        .into_iter()
        .filter(|id| device_has_output_streams(*id))
        .filter_map(device_name)
        .collect()
}
