use std::collections::HashMap;
use std::ffi::c_void;

use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

use super::apply_from_schema;
use crate::game::pages_committed;
use crate::offsets::Offsets;

type CreateInterfaceFn = unsafe extern "system" fn(*const u8, *mut i32) -> *mut c_void;
type FindTypeScopeFn =
    unsafe extern "system" fn(*mut c_void, *const u8, *mut c_void) -> *mut c_void;
type FindDeclaredClassFn = unsafe extern "system" fn(*mut c_void, *mut *mut c_void, *const u8);

struct SchemaRuntime {
    scope: *mut c_void,
    find_class: FindDeclaredClassFn,
    classes: HashMap<String, HashMap<String, u32>>,
}

fn page_checked(address: u64, len: usize) -> Option<&'static [u8]> {
    if !pages_committed(address, len) {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(address as *const u8, len) })
}

fn read_u16(address: u64) -> Option<u16> {
    let bytes = page_checked(address, 2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u32(address: u64) -> Option<u32> {
    let bytes = page_checked(address, 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u64(address: u64) -> Option<u64> {
    let bytes = page_checked(address, 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn read_c_string(address: u64) -> Option<String> {
    if address == 0 {
        return None;
    }
    let bytes = page_checked(address, 64)?;
    let end = bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len());
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

fn wide(name: &str) -> Vec<u8> {
    name.bytes().chain(std::iter::once(0)).collect()
}

impl SchemaRuntime {
    /// Attach to the schema system and client.dll's type scope. Returns None
    /// until schemasystem.dll and client.dll are loaded; callers retry
    fn attach() -> Option<Self> {
        if unsafe { GetModuleHandleA(wide("client.dll").as_ptr()) }.is_null() {
            return None;
        }
        let module = unsafe { GetModuleHandleA(wide("schemasystem.dll").as_ptr()) };
        if module.is_null() {
            return None;
        }
        let factory = unsafe { GetProcAddress(module, wide("CreateInterface").as_ptr()) }?;
        let factory: CreateInterfaceFn = unsafe { std::mem::transmute(factory) };
        let system = unsafe { factory(wide("SchemaSystem_001").as_ptr(), std::ptr::null_mut()) };
        if system.is_null() {
            return None;
        }

        // CSchemaSystem::FindTypeScope(module_name, nullptr): vtable slot 13
        let system_vtable = read_u64(system as u64)?;
        let find_type_scope = unsafe {
            std::mem::transmute::<u64, FindTypeScopeFn>(read_u64(system_vtable + 13 * 8)?)
        };
        let scope =
            unsafe { find_type_scope(system, wide("client.dll").as_ptr(), std::ptr::null_mut()) };
        if scope.is_null() {
            return None;
        }

        // Scope::FindDeclaredClass(out, class_name): vtable slot 2
        let scope_vtable = read_u64(scope as u64)?;
        let find_class = unsafe {
            std::mem::transmute::<u64, FindDeclaredClassFn>(read_u64(scope_vtable + 2 * 8)?)
        };

        Some(Self {
            scope,
            find_class,
            classes: HashMap::new(),
        })
    }

    fn class_fields(&mut self, class: &str) -> Option<&HashMap<String, u32>> {
        if !self.classes.contains_key(class) {
            let fields = self.read_class_fields(class)?;
            self.classes.insert(class.to_string(), fields);
        }
        self.classes.get(class)
    }

    // CSchemaClassInfo layout: field_count i16 @ +0x1C, field list * @ +0x28;
    // entries are 0x20 wide with name char* @ +0x00 and offset i32 @ +0x10
    fn read_class_fields(&self, class: &str) -> Option<HashMap<String, u32>> {
        let mut info: *mut c_void = std::ptr::null_mut();
        unsafe { (self.find_class)(self.scope, &mut info, wide(class).as_ptr()) };
        if info.is_null() {
            return None;
        }
        let info = info as u64;
        let field_count = read_u16(info + 0x1C)? as usize;
        let fields_ptr = read_u64(info + 0x28)?;
        if fields_ptr == 0 || field_count == 0 || field_count > 4096 {
            return None;
        }

        let mut fields = HashMap::with_capacity(field_count);
        for index in 0..field_count {
            let entry = fields_ptr + (index as u64) * 0x20;
            let Some(name_ptr) = read_u64(entry) else {
                continue;
            };
            let Some(name) = read_c_string(name_ptr) else {
                continue;
            };
            let Some(offset) = read_u32(entry + 0x10) else {
                continue;
            };
            fields.insert(name, offset);
        }
        Some(fields)
    }
}

/// Resolve every schema-backed field offset into `offsets`. True when the
/// schema system was reachable and at least a few fields resolved (the
/// caller stops retrying); individual misses keep their baked values
pub fn apply_schema(offsets: &mut Offsets) -> bool {
    let Some(mut runtime) = SchemaRuntime::attach() else {
        return false;
    };
    let applied = apply_from_schema(offsets, |class, field| {
        runtime
            .class_fields(class)
            .and_then(|fields| fields.get(field).copied())
    });
    crate::debug_log::always(&format!("schema runtime: {applied} field offsets resolved"));
    applied >= 8
}
