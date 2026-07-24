use core::mem;
use core::ptr;

use crate::{allocate, deallocate};

#[repr(C)]
pub struct DrMixedV1 {
    pub tag: u8,
    pub type_id: u32,
    pub payload: u64,
    owner: *mut DrMixedOwnerV1,
}

struct DrMixedOwnerV1 {
    references: usize,
    owns_payload: bool,
}

unsafe fn allocate_box(
    tag: u8,
    type_id: u32,
    payload: u64,
    owner: *mut DrMixedOwnerV1,
) -> *mut DrMixedV1 {
    let value = allocate(mem::size_of::<DrMixedV1>()).cast::<DrMixedV1>();
    if value.is_null() {
        return ptr::null_mut();
    }
    ptr::write(
        value,
        DrMixedV1 {
            tag,
            type_id,
            payload,
            owner,
        },
    );
    value
}

pub unsafe fn new_owned(tag: u8, type_id: u32, payload: u64) -> *mut DrMixedV1 {
    let owner = allocate(mem::size_of::<DrMixedOwnerV1>()).cast::<DrMixedOwnerV1>();
    if owner.is_null() {
        return ptr::null_mut();
    }
    ptr::write(
        owner,
        DrMixedOwnerV1 {
            references: 1,
            owns_payload: true,
        },
    );
    let value = allocate_box(tag, type_id, payload, owner);
    if value.is_null() {
        deallocate(owner.cast::<u8>());
    }
    value
}

pub unsafe fn new_borrowed(tag: u8, type_id: u32, payload: u64) -> *mut DrMixedV1 {
    let owner = allocate(mem::size_of::<DrMixedOwnerV1>()).cast::<DrMixedOwnerV1>();
    if owner.is_null() {
        return ptr::null_mut();
    }
    ptr::write(
        owner,
        DrMixedOwnerV1 {
            references: 1,
            owns_payload: false,
        },
    );
    let value = allocate_box(tag, type_id, payload, owner);
    if value.is_null() {
        deallocate(owner.cast::<u8>());
    }
    value
}

pub unsafe fn clone_owned(value: *const DrMixedV1) -> *mut DrMixedV1 {
    if value.is_null() {
        return ptr::null_mut();
    }
    (*(*value).owner).references += 1;
    let clone = allocate_box(
        (*value).tag,
        (*value).type_id,
        (*value).payload,
        (*value).owner,
    );
    if clone.is_null() {
        (*(*value).owner).references -= 1;
    }
    clone
}

pub unsafe fn release_owned(value: *mut DrMixedV1) -> bool {
    if value.is_null() || (*value).owner.is_null() {
        return false;
    }
    let owner = (*value).owner;
    (*owner).references -= 1;
    (*value).owner = ptr::null_mut();
    if (*owner).references != 0 {
        return false;
    }
    let owns_payload = (*owner).owns_payload;
    deallocate(owner.cast::<u8>());
    owns_payload
}

pub unsafe fn free(value: *mut DrMixedV1) {
    if value.is_null() {
        return;
    }
    if !(*value).owner.is_null() {
        let owner = (*value).owner;
        (*owner).references -= 1;
        if (*owner).references == 0 {
            deallocate(owner.cast::<u8>());
        }
    }
    deallocate(value.cast::<u8>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_clones_release_the_payload_only_for_the_final_claim() {
        unsafe {
            let value = new_owned(1, 0, 42);
            let clone = clone_owned(value);
            assert!(!release_owned(value));
            free(value);
            assert!(release_owned(clone));
            free(clone);
        }
    }

    #[test]
    fn borrowed_clones_never_claim_payload_ownership() {
        unsafe {
            let value = new_borrowed(1, 0, 42);
            let clone = clone_owned(value);
            free(value);
            assert!(!release_owned(clone));
            free(clone);
        }
    }
}
