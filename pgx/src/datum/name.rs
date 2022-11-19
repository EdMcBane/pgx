use std::ffi::c_char;
use crate::{pg_sys, FromDatum, IntoDatum};

const NAME_BYTES_LEN: usize = 64;
pub type NameBytes = [u8; NAME_BYTES_LEN];
pub struct Name(NameBytes);

impl FromDatum for Name {
    #[inline]
    unsafe fn from_polymorphic_datum(
        datum: pg_sys::Datum,
        is_null: bool,
        _: pg_sys::Oid,
    ) -> Option<Self> {
        if is_null || datum.is_null() {
            None
        } else {
            let mut name = [0; NAME_BYTES_LEN];
            name.as_mut_ptr().copy_from(datum.cast_mut_ptr(), NAME_BYTES_LEN);
            Some(Name(name))
        }
    }
}

impl IntoDatum for Name {
    #[inline]
    fn into_datum(self) -> Option<pg_sys::Datum> {
        todo!()
    }

    fn type_oid() -> u32 {
        pg_sys::NAMEOID
    }
}

impl Name {
    pub fn as_str(&self) -> &str {
        // # Safety
        // postgres guarantees a name is 64 bytes long and includes a NUL
        unsafe { std::ffi::CStr::from_ptr(self.0.as_ptr() as *const c_char) }
            .to_str()
            .expect("Not utf8")
    }
}