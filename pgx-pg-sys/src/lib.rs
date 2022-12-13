/*
Portions Copyright 2019-2021 ZomboDB, LLC.
Portions Copyright 2021-2022 Technology Concepts & Design, Inc. <support@tcdi.com>

All rights reserved.

Use of this source code is governed by the MIT license that can be found in the LICENSE file.
*/

//
// we allow improper_ctypes just to eliminate these warnings:
//      = note: `#[warn(improper_ctypes)]` on by default
//      = note: 128-bit integers don't currently have a known stable ABI
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(improper_ctypes)]
#![allow(clippy::unneeded_field_pattern)]
#![cfg_attr(nightly, feature(strict_provenance))]

#[cfg(
    any(
        // no features at all will cause problems
        not(any(feature = "pg11", feature = "pg12", feature = "pg13", feature = "pg14", feature = "pg15")),
  ))]
std::compile_error!("exactly one one feature must be provided (pg11, pg12, pg13, pg14, pg15)");

pub mod submodules;

use core::ptr::NonNull;
use std::ffi::CStr;
use std::os::raw::c_char;

// for convenience we pull up everything submodules exposes
pub use submodules::*;

//
// our actual bindings modules -- these are generated by build.rs
//

// feature gate each pg version module
#[cfg(all(feature = "pg11", not(docsrs)))]
mod pg11 {
    include!(concat!(env!("OUT_DIR"), "/pg11.rs"));
}
#[cfg(all(feature = "pg11", docsrs))]
mod pg11;

#[cfg(all(feature = "pg12", not(docsrs)))]
mod pg12 {
    include!(concat!(env!("OUT_DIR"), "/pg12.rs"));
}
#[cfg(all(feature = "pg12", docsrs))]
mod pg12;

#[cfg(all(feature = "pg13", not(docsrs)))]
mod pg13 {
    include!(concat!(env!("OUT_DIR"), "/pg13.rs"));
}
#[cfg(all(feature = "pg13", docsrs))]
mod pg13;

#[cfg(all(feature = "pg14", not(docsrs)))]
mod pg14 {
    include!(concat!(env!("OUT_DIR"), "/pg14.rs"));
}
#[cfg(all(feature = "pg14", docsrs))]
mod pg14;

#[cfg(all(feature = "pg15", not(docsrs)))]
mod pg15 {
    include!(concat!(env!("OUT_DIR"), "/pg15.rs"));
}
#[cfg(all(feature = "pg15", docsrs))]
mod pg15;

// export each module publicly
#[cfg(feature = "pg11")]
pub use pg11::*;
#[cfg(feature = "pg12")]
pub use pg12::*;
#[cfg(feature = "pg13")]
pub use pg13::*;
#[cfg(feature = "pg14")]
pub use pg14::*;
#[cfg(feature = "pg15")]
pub use pg15::*;

// feature gate each pg-specific oid module
#[cfg(all(feature = "pg11", not(docsrs)))]
mod pg11_oids {
    include!(concat!(env!("OUT_DIR"), "/pg11_oids.rs"));
}
#[cfg(all(feature = "pg11", docsrs))]
mod pg11;

#[cfg(all(feature = "pg12", not(docsrs)))]
mod pg12_oids {
    include!(concat!(env!("OUT_DIR"), "/pg12_oids.rs"));
}
#[cfg(all(feature = "pg12", docsrs))]
mod pg12_oids;

#[cfg(all(feature = "pg13", not(docsrs)))]
mod pg13_oids {
    include!(concat!(env!("OUT_DIR"), "/pg13_oids.rs"));
}
#[cfg(all(feature = "pg13", docsrs))]
mod pg13_oids;

#[cfg(all(feature = "pg14", not(docsrs)))]
mod pg14_oids {
    include!(concat!(env!("OUT_DIR"), "/pg14_oids.rs"));
}
#[cfg(all(feature = "pg14", docsrs))]
mod pg14_oids;

#[cfg(all(feature = "pg15", not(docsrs)))]
mod pg15_oids {
    include!(concat!(env!("OUT_DIR"), "/pg15_oids.rs"));
}
#[cfg(all(feature = "pg15", docsrs))]
mod pg15_oids;

// export that module publicly
#[cfg(feature = "pg11")]
pub use pg11_oids::*;
#[cfg(feature = "pg12")]
pub use pg12_oids::*;
#[cfg(feature = "pg13")]
pub use pg13_oids::*;
#[cfg(feature = "pg14")]
pub use pg14_oids::*;
#[cfg(feature = "pg15")]
pub use pg15_oids::*;

// expose things we want available for all versions
pub use all_versions::*;

// and things that are version-specific
#[cfg(feature = "pg11")]
pub use internal::pg11::IndexBuildHeapScan;
#[cfg(feature = "pg11")]
pub use internal::pg11::*;

#[cfg(feature = "pg12")]
pub use internal::pg12::*;

#[cfg(feature = "pg13")]
pub use internal::pg13::*;

#[cfg(feature = "pg14")]
pub use internal::pg14::*;

#[cfg(feature = "pg15")]
pub use internal::pg15::*;

/// A trait applied to all Postgres `pg_sys::Node` types and subtypes
pub trait PgNode: seal::Sealed {
    /// Format this node
    #[inline]
    fn display_node(&self) -> std::string::String {
        // SAFETY: The trait is pub but this impl is private, and
        // this is only implemented for things known to be "Nodes"
        unsafe { display_node_impl(NonNull::from(self).cast()) }
    }
}

mod seal {
    pub trait Sealed {}
}

/// implementation function for `impl Display for $NodeType`
///
/// # Safety
/// Don't use this on anything that doesn't impl PgNode, or the type may be off
#[warn(unsafe_op_in_unsafe_fn)]
pub(crate) unsafe fn display_node_impl(node: NonNull<crate::Node>) -> std::string::String {
    // SAFETY: It's fine to call nodeToString with non-null well-typed pointers,
    // and pg_sys::nodeToString() returns data via palloc, which is never null
    // as Postgres will ERROR rather than giving us a null pointer,
    // and Postgres starts and finishes constructing StringInfos by writing '\0'
    unsafe {
        let node_cstr = crate::nodeToString(node.as_ptr().cast());

        let result = match CStr::from_ptr(node_cstr).to_str() {
            Ok(cstr) => cstr.to_string(),
            Err(e) => format!("<ffi error: {:?}>", e),
        };

        crate::pfree(node_cstr.cast());

        result
    }
}

/// A trait for converting a thing into a `char *` that is allocated by Postgres' palloc
pub trait AsPgCStr {
    /// Consumes `self` and converts it into a Postgres-allocated `char *`
    fn as_pg_cstr(self) -> *mut std::os::raw::c_char;
}

impl<'a> AsPgCStr for &'a str {
    fn as_pg_cstr(self) -> *mut std::os::raw::c_char {
        let self_bytes = self.as_bytes();
        let pg_cstr = unsafe { crate::palloc0(self_bytes.len() + 1) as *mut std::os::raw::c_uchar };
        let slice = unsafe { std::slice::from_raw_parts_mut(pg_cstr, self_bytes.len()) };
        slice.copy_from_slice(self_bytes);
        pg_cstr as *mut std::os::raw::c_char
    }
}

impl<'a> AsPgCStr for Option<&'a str> {
    fn as_pg_cstr(self) -> *mut c_char {
        match self {
            Some(s) => s.as_pg_cstr(),
            None => std::ptr::null_mut(),
        }
    }
}

impl AsPgCStr for std::string::String {
    fn as_pg_cstr(self) -> *mut std::os::raw::c_char {
        self.as_str().as_pg_cstr()
    }
}

impl AsPgCStr for &std::string::String {
    fn as_pg_cstr(self) -> *mut std::os::raw::c_char {
        self.as_str().as_pg_cstr()
    }
}

impl AsPgCStr for Option<std::string::String> {
    fn as_pg_cstr(self) -> *mut c_char {
        match self {
            Some(s) => s.as_pg_cstr(),
            None => std::ptr::null_mut(),
        }
    }
}

impl AsPgCStr for Option<&std::string::String> {
    fn as_pg_cstr(self) -> *mut c_char {
        match self {
            Some(s) => s.as_pg_cstr(),
            None => std::ptr::null_mut(),
        }
    }
}

impl AsPgCStr for &Option<std::string::String> {
    fn as_pg_cstr(self) -> *mut c_char {
        match self {
            Some(s) => s.as_pg_cstr(),
            None => std::ptr::null_mut(),
        }
    }
}

/// item declarations we want to add to all versions
mod all_versions {
    use crate as pg_sys;
    use pgx_macros::*;

    use memoffset::*;
    use std::str::FromStr;

    pub use crate::submodules::htup::*;

    /// this comes from `postgres_ext.h`
    pub const InvalidOid: super::Oid = 0;
    pub const InvalidOffsetNumber: super::OffsetNumber = 0;
    pub const FirstOffsetNumber: super::OffsetNumber = 1;
    pub const MaxOffsetNumber: super::OffsetNumber =
        (super::BLCKSZ as usize / std::mem::size_of::<super::ItemIdData>()) as super::OffsetNumber;
    pub const InvalidBlockNumber: u32 = 0xFFFF_FFFF as crate::BlockNumber;
    pub const VARHDRSZ: usize = std::mem::size_of::<super::int32>();
    pub const InvalidTransactionId: super::TransactionId = 0 as super::TransactionId;
    pub const InvalidCommandId: super::CommandId = (!(0 as super::CommandId)) as super::CommandId;
    pub const FirstCommandId: super::CommandId = 0 as super::CommandId;
    pub const BootstrapTransactionId: super::TransactionId = 1 as super::TransactionId;
    pub const FrozenTransactionId: super::TransactionId = 2 as super::TransactionId;
    pub const FirstNormalTransactionId: super::TransactionId = 3 as super::TransactionId;
    pub const MaxTransactionId: super::TransactionId = 0xFFFF_FFFF as super::TransactionId;

    #[pgx_macros::pg_guard]
    extern "C" {
        pub fn pgx_list_nth(list: *mut super::List, nth: i32) -> *mut std::os::raw::c_void;
        pub fn pgx_list_nth_int(list: *mut super::List, nth: i32) -> i32;
        pub fn pgx_list_nth_oid(list: *mut super::List, nth: i32) -> super::Oid;
        pub fn pgx_list_nth_cell(list: *mut super::List, nth: i32) -> *mut super::ListCell;
    }

    /// Given a valid HeapTuple pointer, return address of the user data
    ///
    /// # Safety
    ///
    /// This function cannot determine if the `tuple` argument is really a non-null pointer to a [`HeapTuple`].
    #[inline(always)]
    pub unsafe fn GETSTRUCT(tuple: crate::HeapTuple) -> *mut std::os::raw::c_char {
        // #define GETSTRUCT(TUP) ((char *) ((TUP)->t_data) + (TUP)->t_data->t_hoff)

        // SAFETY:  The caller has asserted `tuple` is a valid HeapTuple and is properly aligned
        // Additionally, t_data.t_hoff is an a u8, so it'll fit inside a usize
        (*tuple).t_data.cast::<std::os::raw::c_char>().add((*(*tuple).t_data).t_hoff as _)
    }

    #[inline]
    pub fn VARHDRSZ_EXTERNAL() -> usize {
        offset_of!(super::varattrib_1b_e, va_data)
    }

    #[inline]
    pub fn VARHDRSZ_SHORT() -> usize {
        offset_of!(super::varattrib_1b, va_data)
    }

    #[inline]
    pub fn get_pg_major_version_string() -> &'static str {
        let mver = std::ffi::CStr::from_bytes_with_nul(super::PG_MAJORVERSION).unwrap();
        mver.to_str().unwrap()
    }

    #[inline]
    pub fn get_pg_major_version_num() -> u16 {
        u16::from_str(super::get_pg_major_version_string()).unwrap()
    }

    #[inline]
    pub fn get_pg_version_string() -> &'static str {
        let ver = std::ffi::CStr::from_bytes_with_nul(super::PG_VERSION_STR).unwrap();
        ver.to_str().unwrap()
    }

    #[inline]
    pub fn get_pg_major_minor_version_string() -> &'static str {
        let mver = std::ffi::CStr::from_bytes_with_nul(super::PG_VERSION).unwrap();
        mver.to_str().unwrap()
    }

    #[inline]
    pub fn TransactionIdIsNormal(xid: super::TransactionId) -> bool {
        xid >= FirstNormalTransactionId
    }

    /// ```c
    ///     #define type_is_array(typid)  (get_element_type(typid) != InvalidOid)
    /// ```
    #[inline]
    pub unsafe fn type_is_array(typoid: super::Oid) -> bool {
        super::get_element_type(typoid) != InvalidOid
    }

    #[inline]
    pub unsafe fn planner_rt_fetch(
        index: super::Index,
        root: *mut super::PlannerInfo,
    ) -> *mut super::RangeTblEntry {
        extern "C" {
            pub fn pgx_planner_rt_fetch(
                index: super::Index,
                root: *mut super::PlannerInfo,
            ) -> *mut super::RangeTblEntry;
        }

        pgx_planner_rt_fetch(index, root)
    }

    /// ```c
    /// #define rt_fetch(rangetable_index, rangetable) \
    ///     ((RangeTblEntry *) list_nth(rangetable, (rangetable_index)-1))
    /// ```
    #[inline]
    pub unsafe fn rt_fetch(
        index: super::Index,
        range_table: *mut super::List,
    ) -> *mut super::RangeTblEntry {
        pgx_list_nth(range_table, index as i32 - 1) as *mut super::RangeTblEntry
    }

    /// #define BufferGetPage(buffer) ((Page)BufferGetBlock(buffer))
    #[inline]
    pub unsafe fn BufferGetPage(buffer: crate::Buffer) -> crate::Page {
        BufferGetBlock(buffer) as crate::Page
    }

    /// #define BufferGetBlock(buffer) \
    /// ( \
    ///      AssertMacro(BufferIsValid(buffer)), \
    ///      BufferIsLocal(buffer) ? \
    ///            LocalBufferBlockPointers[-(buffer) - 1] \
    ///      : \
    ///            (Block) (BufferBlocks + ((Size) ((buffer) - 1)) * BLCKSZ) \
    /// )
    #[inline]
    pub unsafe fn BufferGetBlock(buffer: crate::Buffer) -> crate::Block {
        if BufferIsLocal(buffer) {
            *crate::LocalBufferBlockPointers.offset(((-buffer) - 1) as isize)
        } else {
            crate::BufferBlocks
                .offset((((buffer as crate::Size) - 1) * crate::BLCKSZ as usize) as isize)
                as crate::Block
        }
    }

    /// #define BufferIsLocal(buffer)      ((buffer) < 0)
    #[inline]
    pub unsafe fn BufferIsLocal(buffer: crate::Buffer) -> bool {
        buffer < 0
    }

    /// Retrieve the "user data" of the specified [`HeapTuple`] as a specific type. Typically this
    /// will be a struct that represents a Postgres system catalog, such as [`FormData_pg_class`].
    ///
    /// # Returns
    ///
    /// A pointer to the [`HeapTuple`]'s "user data", cast as a mutable pointer to `T`.  If the
    /// specified `htup` pointer is null, the null pointer is returned.
    ///
    /// # Safety
    ///
    /// This function cannot verify that the specified `htup` points to a valid [`HeapTuple`] nor
    /// that if it does, that its bytes are bitwise compatible with `T`.
    #[inline]
    pub unsafe fn heap_tuple_get_struct<T>(htup: super::HeapTuple) -> *mut T {
        if htup.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe {
                // SAFETY:  The caller has told us `htop` is a valid HeapTuple
                GETSTRUCT(htup).cast()
            }
        }
    }

    #[pg_guard]
    extern "C" {
        pub fn query_tree_walker(
            query: *mut super::Query,
            walker: ::std::option::Option<
                unsafe extern "C" fn(*mut super::Node, *mut ::std::os::raw::c_void) -> bool,
            >,
            context: *mut ::std::os::raw::c_void,
            flags: ::std::os::raw::c_int,
        ) -> bool;
    }

    #[pg_guard]
    extern "C" {
        pub fn expression_tree_walker(
            node: *mut super::Node,
            walker: ::std::option::Option<
                unsafe extern "C" fn(*mut super::Node, *mut ::std::os::raw::c_void) -> bool,
            >,
            context: *mut ::std::os::raw::c_void,
        ) -> bool;
    }

    #[pgx_macros::pg_guard]
    extern "C" {
        #[link_name = "pgx_SpinLockInit"]
        pub fn SpinLockInit(lock: *mut pg_sys::slock_t);
        #[link_name = "pgx_SpinLockAcquire"]
        pub fn SpinLockAcquire(lock: *mut pg_sys::slock_t);
        #[link_name = "pgx_SpinLockRelease"]
        pub fn SpinLockRelease(lock: *mut pg_sys::slock_t);
        #[link_name = "pgx_SpinLockFree"]
        pub fn SpinLockFree(lock: *mut pg_sys::slock_t) -> bool;
    }

    #[inline(always)]
    pub unsafe fn MemoryContextSwitchTo(context: crate::MemoryContext) -> crate::MemoryContext {
        let old = crate::CurrentMemoryContext;

        crate::CurrentMemoryContext = context;
        old
    }
}

mod internal {
    //
    // for specific versions
    //
    #[cfg(feature = "pg11")]
    pub(crate) mod pg11 {
        pub use crate::pg11::tupleDesc as TupleDescData;
        pub type QueryCompletion = std::os::raw::c_char;

        /// # Safety
        ///
        /// This function wraps Postgres' internal `IndexBuildHeapScan` method, and therefore, is
        /// inherently unsafe
        pub unsafe fn IndexBuildHeapScan<T>(
            heap_relation: crate::Relation,
            index_relation: crate::Relation,
            index_info: *mut crate::pg11::IndexInfo,
            build_callback: crate::IndexBuildCallback,
            build_callback_state: *mut T,
        ) {
            crate::pg11::IndexBuildHeapScan(
                heap_relation,
                index_relation,
                index_info,
                true,
                build_callback,
                build_callback_state as *mut std::os::raw::c_void,
                std::ptr::null_mut(),
            );
        }
    }

    #[cfg(feature = "pg12")]
    pub(crate) mod pg12 {
        pub use crate::pg12::AllocSetContextCreateInternal as AllocSetContextCreateExtended;
        pub type QueryCompletion = std::os::raw::c_char;

        pub const QTW_EXAMINE_RTES: u32 = crate::pg12::QTW_EXAMINE_RTES_BEFORE;

        /// # Safety
        ///
        /// This function wraps Postgres' internal `IndexBuildHeapScan` method, and therefore, is
        /// inherently unsafe
        pub unsafe fn IndexBuildHeapScan<T>(
            heap_relation: crate::Relation,
            index_relation: crate::Relation,
            index_info: *mut crate::pg12::IndexInfo,
            build_callback: crate::IndexBuildCallback,
            build_callback_state: *mut T,
        ) {
            let heap_relation_ref = heap_relation.as_ref().unwrap();
            let table_am = heap_relation_ref.rd_tableam.as_ref().unwrap();

            table_am.index_build_range_scan.unwrap()(
                heap_relation,
                index_relation,
                index_info,
                true,
                false,
                true,
                0,
                crate::InvalidBlockNumber,
                build_callback,
                build_callback_state as *mut std::os::raw::c_void,
                std::ptr::null_mut(),
            );
        }
    }

    #[cfg(feature = "pg13")]
    pub(crate) mod pg13 {
        pub use crate::pg13::AllocSetContextCreateInternal as AllocSetContextCreateExtended;

        pub const QTW_EXAMINE_RTES: u32 = crate::pg13::QTW_EXAMINE_RTES_BEFORE;

        /// # Safety
        ///
        /// This function wraps Postgres' internal `IndexBuildHeapScan` method, and therefore, is
        /// inherently unsafe
        pub unsafe fn IndexBuildHeapScan<T>(
            heap_relation: crate::Relation,
            index_relation: crate::Relation,
            index_info: *mut crate::IndexInfo,
            build_callback: crate::IndexBuildCallback,
            build_callback_state: *mut T,
        ) {
            let heap_relation_ref = heap_relation.as_ref().unwrap();
            let table_am = heap_relation_ref.rd_tableam.as_ref().unwrap();

            table_am.index_build_range_scan.unwrap()(
                heap_relation,
                index_relation,
                index_info,
                true,
                false,
                true,
                0,
                crate::InvalidBlockNumber,
                build_callback,
                build_callback_state as *mut std::os::raw::c_void,
                std::ptr::null_mut(),
            );
        }
    }

    #[cfg(feature = "pg14")]
    pub(crate) mod pg14 {
        pub use crate::pg14::AllocSetContextCreateInternal as AllocSetContextCreateExtended;

        pub const QTW_EXAMINE_RTES: u32 = crate::pg14::QTW_EXAMINE_RTES_BEFORE;

        /// # Safety
        ///
        /// This function wraps Postgres' internal `IndexBuildHeapScan` method, and therefore, is
        /// inherently unsafe
        pub unsafe fn IndexBuildHeapScan<T>(
            heap_relation: crate::Relation,
            index_relation: crate::Relation,
            index_info: *mut crate::IndexInfo,
            build_callback: crate::IndexBuildCallback,
            build_callback_state: *mut T,
        ) {
            let heap_relation_ref = heap_relation.as_ref().unwrap();
            let table_am = heap_relation_ref.rd_tableam.as_ref().unwrap();

            table_am.index_build_range_scan.unwrap()(
                heap_relation,
                index_relation,
                index_info,
                true,
                false,
                true,
                0,
                crate::InvalidBlockNumber,
                build_callback,
                build_callback_state as *mut std::os::raw::c_void,
                std::ptr::null_mut(),
            );
        }
    }

    #[cfg(feature = "pg15")]
    pub(crate) mod pg15 {
        pub use crate::pg15::AllocSetContextCreateInternal as AllocSetContextCreateExtended;

        pub const QTW_EXAMINE_RTES: u32 = crate::pg15::QTW_EXAMINE_RTES_BEFORE;

        /// # Safety
        ///
        /// This function wraps Postgres' internal `IndexBuildHeapScan` method, and therefore, is
        /// inherently unsafe
        pub unsafe fn IndexBuildHeapScan<T>(
            heap_relation: crate::Relation,
            index_relation: crate::Relation,
            index_info: *mut crate::IndexInfo,
            build_callback: crate::IndexBuildCallback,
            build_callback_state: *mut T,
        ) {
            let heap_relation_ref = heap_relation.as_ref().unwrap();
            let table_am = heap_relation_ref.rd_tableam.as_ref().unwrap();

            table_am.index_build_range_scan.unwrap()(
                heap_relation,
                index_relation,
                index_info,
                true,
                false,
                true,
                0,
                crate::InvalidBlockNumber,
                build_callback,
                build_callback_state as *mut std::os::raw::c_void,
                std::ptr::null_mut(),
            );
        }
    }
}

// Hack to fix linker errors that we get under amazonlinux2 on some PG versions
// due to our wrappers for various system library functions. Should be fairly
// harmless, but ideally we would not wrap these functions
// (https://github.com/tcdi/pgx/issues/730).
#[cfg(target_os = "linux")]
#[link(name = "resolv")]
extern "C" {}
