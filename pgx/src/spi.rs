/*
Portions Copyright 2019-2021 ZomboDB, LLC.
Portions Copyright 2021-2022 Technology Concepts & Design, Inc. <support@tcdi.com>

All rights reserved.

Use of this source code is governed by the MIT license that can be found in the LICENSE file.
*/

//! Safe access to Postgres' *Server Programming Interface* (SPI).

use crate::{pg_sys, FromDatum, IntoDatum, Json, PgMemoryContexts, PgOid};
use enum_primitive_derive::*;
use num_traits::FromPrimitive;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::{Index, IndexMut};

#[derive(Debug, Primitive)]
pub enum SpiOk {
    Connect = 1,
    Finish = 2,
    Fetch = 3,
    Utility = 4,
    Select = 5,
    Selinto = 6,
    Insert = 7,
    Delete = 8,
    Update = 9,
    Cursor = 10,
    InsertReturning = 11,
    DeleteReturning = 12,
    UpdateReturning = 13,
    Rewritten = 14,
    RelRegister = 15,
    RelUnregister = 16,
    TdRegister = 17,
}

#[derive(Debug, Primitive)]
pub enum SpiError {
    // NB:  These are #define'd as negative, but we redefine them as positive so that
    // #[derive(Primitive)] will work.  We just need to negate result codes from the
    // various SPI_xxx functions when looking for errors
    Connect = 1,
    Copy = 2,
    Opunknown = 3,
    Unconnected = 4,
    #[allow(dead_code)]
    Cursor = 5, /* not used anymore */
    Argument = 6,
    Param = 7,
    Transaction = 8,
    Noattribute = 9,
    Nooutfunc = 10,
    Typunknown = 11,
    RelDuplicate = 12,
    RelNotFound = 13,
}

pub struct Spi;

#[derive(Debug)]
pub struct SpiClient;

#[derive(Debug)]
pub struct SpiTupleTable<'client> {
    #[allow(dead_code)]
    status_code: SpiOk,
    table: *mut pg_sys::SPITupleTable,
    size: usize,
    tupdesc: Option<pg_sys::TupleDesc>,
    current: isize,
    _client: &'client SpiClient,
}

impl Drop for SpiTupleTable<'_> {
    fn drop(&mut self) {
        unsafe {
            info!("drop");
            pg_sys::SPI_freetuptable(self.table);
        }
    }
}

/// Represents a single `pg_sys::Datum` inside a `SpiHeapTupleData`
pub struct SpiHeapTupleDataEntry<'t> {
    datum: Option<pg_sys::Datum>,
    type_oid: pg_sys::Oid,
    _table: &'t SpiTupleTable<'t>,
}

/// Represents the set of `pg_sys::Datum`s in a `pg_sys::HeapTuple`
pub struct SpiHeapTupleData<'t> {
    tupdesc: pg_sys::TupleDesc,
    entries: HashMap<usize, SpiHeapTupleDataEntry<'t>>,
    _table: &'t SpiTupleTable<'t>,
}

impl Spi {
    pub fn get_one<A: FromDatum + IntoDatum>(query: &str) -> Option<A> {
        Spi::connect(|client| {
            let result = client.select(query, Some(1), None).first().get_one();
            info!("now yield");
            Ok(result)
        })
    }

    pub fn get_two<A: FromDatum + IntoDatum, B: FromDatum + IntoDatum>(
        query: &str,
    ) -> (Option<A>, Option<B>) {
        Spi::connect(|client| {
            let (a, b) = client
                .select(query, Some(1), None)
                .first()
                .get_two::<A, B>();
            Ok(Some((a, b)))
        })
            .unwrap()
    }

    pub fn get_three<
        A: FromDatum + IntoDatum,
        B: FromDatum + IntoDatum,
        C: FromDatum + IntoDatum,
    >(
        query: &str,
    ) -> (Option<A>, Option<B>, Option<C>) {
        Spi::connect(|client| {
            let (a, b, c) = client
                .select(query, Some(1), None)
                .first()
                .get_three::<A, B, C>();
            Ok(Some((a, b, c)))
        })
            .unwrap()
    }

    pub fn get_one_with_args<A: FromDatum + IntoDatum>(
        query: &str,
        args: Vec<(PgOid, Option<pg_sys::Datum>)>,
    ) -> Option<A> {
        Spi::connect(|client| Ok(client.select(query, Some(1), Some(args)).first().get_one()))
    }

    pub fn get_two_with_args<A: FromDatum + IntoDatum, B: FromDatum + IntoDatum>(
        query: &str,
        args: Vec<(PgOid, Option<pg_sys::Datum>)>,
    ) -> (Option<A>, Option<B>) {
        Spi::connect(|client| {
            let (a, b) = client
                .select(query, Some(1), Some(args))
                .first()
                .get_two::<A, B>();
            Ok(Some((a, b)))
        })
            .unwrap()
    }

    pub fn get_three_with_args<
        A: FromDatum + IntoDatum,
        B: FromDatum + IntoDatum,
        C: FromDatum + IntoDatum,
    >(
        query: &str,
        args: Vec<(PgOid, Option<pg_sys::Datum>)>,
    ) -> (Option<A>, Option<B>, Option<C>) {
        Spi::connect(|client| {
            let (a, b, c) = client
                .select(query, Some(1), Some(args))
                .first()
                .get_three::<A, B, C>();
            Ok(Some((a, b, c)))
        })
            .unwrap()
    }

    /// just run an arbitrary SQL statement.
    ///
    /// ## Safety
    ///
    /// The statement runs in read/write mode
    pub fn run(query: &str) {
        Spi::execute(|client| {
            client.update(query, None, None);
        })
    }

    /// explain a query, returning its result in json form
    pub fn explain(query: &str) -> Json {
        Spi::connect(|client| {
            let table = client
                .update(&format!("EXPLAIN (format json) {}", query), None, None)
                .first();
            Ok(Some(
                table
                    .get_one::<Json>()
                    .expect("failed to get json EXPLAIN result"),
            ))
        })
            .unwrap()
    }

    /// execute SPI commands via the provided `SpiClient`
    pub fn execute<F: FnOnce(SpiClient) + std::panic::UnwindSafe>(f: F) {
        Spi::connect(|client| {
            f(client);
            Ok(Some(()))
        });
    }

    /// execute SPI commands via the provided `SpiClient` and return a value from SPI which is
    /// automatically copied into the `CurrentMemoryContext` at the time of this function call
    pub fn connect<
        R: FromDatum + IntoDatum,
        F: FnOnce(SpiClient) -> std::result::Result<Option<R>, SpiError>,
    >(
        f: F,
    ) -> Option<R> {
        let outer_memory_context =
            PgMemoryContexts::For(PgMemoryContexts::CurrentMemoryContext.value());

        /// a struct to manage our SPI connection lifetime
        struct SpiConnection;
        impl SpiConnection {
            /// Connect to Postgres' SPI system
            fn connect() -> Self {
                // connect to SPI
                Spi::check_status(unsafe { pg_sys::SPI_connect() });
                SpiConnection
            }
        }

        impl Drop for SpiConnection {
            /// when SpiConnection is dropped, we make sure to disconnect from SPI
            fn drop(&mut self) {
                // disconnect from SPI
                Spi::check_status(unsafe { pg_sys::SPI_finish() });
            }
        }

        // connect to SPI
        let _connection = SpiConnection::connect();

        // run the provided closure within the memory context that SPI_connect()
        // just put us un.  We'll disconnect from SPI when the closure is finished.
        // If there's a panic or elog(ERROR), we don't care about also disconnecting from
        // SPI b/c Postgres will do that for us automatically
        match f(SpiClient) {
            // copy the result to the outer memory context we saved above
            Ok(result) => {
                // we need to copy the resulting Datum into the outer memory context
                // *before* we disconnect from SPI, otherwise we're copying free'd memory
                // see https://github.com/zombodb/pgx/issues/17
                let copied_datum = match result {
                    Some(result) => {
                        let as_datum = result.into_datum();
                        if as_datum.is_none() {
                            // SPI function returned Some(()), which means we just want to return None
                            None
                        } else {
                            unsafe {
                                R::from_datum_in_memory_context(
                                    outer_memory_context,
                                    as_datum.expect("SPI result datum was NULL"),
                                    false,
                                    pg_sys::InvalidOid,
                                )
                            }
                        }
                    }
                    None => None,
                };

                copied_datum
            }

            // closure returned an error
            Err(e) => panic!("{:?}", e),
        }
    }

    pub fn check_status(status_code: i32) -> SpiOk {
        if status_code > 0 {
            let status_enum = SpiOk::from_i32(status_code);
            match status_enum {
                Some(ok) => ok,
                None => panic!("unrecognized SPI status code {}", status_code),
            }
        } else {
            let status_enum = SpiError::from_i32(-status_code);
            match status_enum {
                Some(e) => panic!("{:?}", e),
                None => panic!("unrecognized SPI status code {}", status_code),
            }
        }
    }
}

impl SpiClient {
    /// perform a SELECT statement
    pub fn select<'t, 's>(
        &'t self,
        query: &'s str,
        limit: Option<i64>,
        args: Option<Vec<(PgOid, Option<pg_sys::Datum>)>>,
    ) -> SpiTupleTable<'t> {
        // Postgres docs say:
        //
        //    It is generally unwise to mix read-only and read-write commands within a single function
        //    using SPI; that could result in very confusing behavior, since the read-only queries
        //    would not see the results of any database updates done by the read-write queries.
        //
        // As such, we don't actually set read-only to true here

        // TODO:  can we detect if the command counter (or something?) has incremented and if yes
        //        then we set read_only=false, else we can set it to true?
        //        Is this even a good idea?
        SpiClient::execute(self, query, false, limit, args)
    }

    /// perform any query (including utility statements) that modify the database in some way
    pub fn update(
        &self,
        query: &str,
        limit: Option<i64>,
        args: Option<Vec<(PgOid, Option<pg_sys::Datum>)>>,
    ) -> SpiTupleTable {
        SpiClient::execute(self, query, false, limit, args)
    }

    fn execute<'c, 's>(
        client: &'c SpiClient,
        query: &'s str,
        read_only: bool,
        limit: Option<i64>,
        args: Option<Vec<(PgOid, Option<pg_sys::Datum>)>>,
    ) -> SpiTupleTable<'c> {
        unsafe {
            pg_sys::SPI_tuptable = std::ptr::null_mut();
        }

        let src = std::ffi::CString::new(query).expect("query contained a null byte");
        let status_code = match args {
            Some(args) => {
                let nargs = args.len();
                let mut argtypes = vec![];
                let mut datums = vec![];
                let mut nulls = vec![];

                for (argtype, datum) in args {
                    argtypes.push(argtype.value());

                    match datum {
                        Some(datum) => {
                            // ' ' here means that the datum is not null
                            datums.push(datum);
                            nulls.push(' ' as std::os::raw::c_char);
                        }

                        None => {
                            // 'n' here means that the datum is null
                            datums.push(0);
                            nulls.push('n' as std::os::raw::c_char);
                        }
                    }
                }

                unsafe {
                    pg_sys::SPI_execute_with_args(
                        src.as_ptr(),
                        nargs as i32,
                        argtypes.as_mut_ptr(),
                        datums.as_mut_ptr(),
                        nulls.as_ptr(),
                        read_only,
                        limit.unwrap_or(0),
                    )
                }
            }
            None => unsafe { pg_sys::SPI_execute(src.as_ptr(), read_only, limit.unwrap_or(0)) },
        };

        SpiTupleTable {
            status_code: Spi::check_status(status_code),
            table: unsafe { pg_sys::SPI_tuptable },
            size: unsafe { pg_sys::SPI_processed as usize },
            tupdesc: if unsafe { pg_sys::SPI_tuptable }.is_null() {
                None
            } else {
                Some(unsafe { (*pg_sys::SPI_tuptable).tupdesc })
            },
            current: -1,
            _client: client,
        }
    }

    /// sets up a cursor that will execute the specified query.
    /// Rows may be fetched then using [`SpiCursor::fetch`]
    pub fn open_cursor(
        &self,
        query: &str,
        args: Option<Vec<(PgOid, Option<pg_sys::Datum>)>>,
    ) -> SpiCursor {
        let src = std::ffi::CString::new(query).expect("query contained a null byte");
        let args = args.unwrap_or(vec![]);

        let nargs = args.len();
        let mut argtypes = vec![];
        let mut datums = vec![];
        let mut nulls = vec![];

        for (argtype, datum) in args {
            argtypes.push(argtype.value());

            match datum {
                Some(datum) => {
                    // ' ' here means that the datum is not null
                    datums.push(datum);
                    nulls.push(' ' as std::os::raw::c_char);
                }

                None => {
                    // 'n' here means that the datum is null
                    datums.push(0);
                    nulls.push('n' as std::os::raw::c_char);
                }
            }
        }

        let ptr = unsafe {
            pg_sys::SPI_cursor_open_with_args(
                std::ptr::null_mut(), // let postgres assign a name
                src.as_ptr(),
                nargs as i32,
                argtypes.as_mut_ptr(),
                datums.as_mut_ptr(),
                nulls.as_ptr(),
                false,
                0,
            )
        };
        SpiCursor { ptr, client: self }
    }

    /// find a cursor in transaction by name. Name should usually be retrieved via [`SpiCursor::into_name`]
    pub fn find_cursor(
        &mut self,
        name: String,
    ) -> SpiCursor {
        use pgx_pg_sys::AsPgCStr;

        let ptr = unsafe {
            pg_sys::SPI_cursor_find(name.as_pg_cstr())
        };
        if ptr.is_null() {
            error!("cursor named \"{}\" not found", name);
        }
        SpiCursor { ptr, client: self }
    }
}


pub struct SpiCursor<'client> {
    ptr: pg_sys::Portal,
    client: &'client SpiClient,
}

impl SpiCursor<'_> {
    /// fetch some rows from a cursor
    pub fn fetch(
        &mut self,
        count: i64
    ) -> SpiTupleTable {
        unsafe {
            pg_sys::SPI_tuptable = std::ptr::null_mut();
        }
        unsafe {
            pg_sys::SPI_cursor_fetch(
                self.ptr,
                true,
                count,
            )
        }
        SpiTupleTable {
            status_code: SpiOk::Fetch,
            table: unsafe { pg_sys::SPI_tuptable },
            size: unsafe { pg_sys::SPI_processed as usize },
            tupdesc: if unsafe { pg_sys::SPI_tuptable }.is_null() {
                None
            } else {
                Some(unsafe { (*pg_sys::SPI_tuptable).tupdesc })
            },
            current: -1,
            _client: &self.client,
        }
    }

    /// consumes the cursor and returns its name.
    /// This allows to fetch it in a later SPI session within the same transaction
    /// using [`SpiClient::find_cursor()`]
    pub fn into_name(self) -> String {
        unsafe { std::ffi::CStr::from_ptr((*self.ptr).name) }
            .to_str().expect("non-utf8 cursor name").to_string()
    }

    /// closes the cursor and releases its resources.
    /// Not mandatory, as all open cursors are closed automatically at the end of a transaction.
    pub fn close(mut self) {
        let ptr = std::mem::replace(&mut self.ptr, std::ptr::null_mut());
        unsafe {
            pg_sys::SPI_cursor_close(ptr)
        }
    }
}


impl SpiTupleTable<'_> {
    /// `SpiTupleTable`s are positioned before the start, for iteration purposes.
    ///
    /// This method moves the position to the first row.  If there are no rows, this
    /// method will silently return.
    pub fn first(mut self) -> Self {
        self.current = 0;
        self
    }

    /// How many rows were processed?
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get_one<A: FromDatum>(&self) -> Option<A> {
        info!("now get");
        self.get_datum(1)
    }

    pub fn get_two<A: FromDatum, B: FromDatum>(&self) -> (Option<A>, Option<B>) {
        let a = self.get_datum::<A>(1);
        let b = self.get_datum::<B>(2);
        (a, b)
    }

    pub fn get_three<A: FromDatum, B: FromDatum, C: FromDatum>(
        &self,
    ) -> (Option<A>, Option<B>, Option<C>) {
        let a = self.get_datum::<A>(1);
        let b = self.get_datum::<B>(2);
        let c = self.get_datum::<C>(3);
        (a, b, c)
    }

    pub fn get_heap_tuple(&self) -> Option<SpiHeapTupleData> {
        if self.current < 0 {
            panic!("SpiTupleTable positioned before start")
        }
        if self.current as u64 >= unsafe { pg_sys::SPI_processed } {
            None
        } else {
            match self.tupdesc {
                Some(tupdesc) => unsafe {
                    let heap_tuple = std::slice::from_raw_parts((*self.table).vals, self.size)
                        [self.current as usize];

                    // SAFETY:  we know heap_tuple is valid because we just made it
                    Some(SpiHeapTupleData::new(self, tupdesc, heap_tuple))
                },
                None => panic!("TupDesc is NULL"),
            }
        }
    }

    pub fn get_datum<T: FromDatum>(&self, ordinal: i32) -> Option<T> {
        if self.current < 0 {
            panic!("SpiTupleTable positioned before start")
        }
        if self.current as u64 >= unsafe { pg_sys::SPI_processed } {
            None
        } else {
            match self.tupdesc {
                Some(tupdesc) => unsafe {
                    let natts = (*tupdesc).natts;

                    if ordinal < 1 || ordinal > natts {
                        None
                    } else {
                        let heap_tuple = std::slice::from_raw_parts((*self.table).vals, self.size)
                            [self.current as usize];
                        let mut is_null = false;
                        let datum =
                            pg_sys::SPI_getbinval(heap_tuple, tupdesc, ordinal, &mut is_null);

                        T::from_datum(datum, is_null, pg_sys::SPI_gettypeid(tupdesc, ordinal))
                    }
                },
                None => panic!("TupDesc is NULL"),
            }
        }
    }
    pub fn iter(&self) -> SpiTupleTableIter {
        SpiTupleTableIter {
            table: self,
            current: 0,
        }
    }
}

impl<'t> SpiHeapTupleData<'t> {
    /// Create a new `SpiHeapTupleData` from its constituent parts
    pub unsafe fn new(table: &'t SpiTupleTable, tupdesc: pg_sys::TupleDesc, htup: *mut pg_sys::HeapTupleData) -> Self {
        let mut data = SpiHeapTupleData {
            tupdesc,
            entries: HashMap::default(),
            _table: table,
        };

        for i in 1..=tupdesc.as_ref().unwrap().natts {
            let mut is_null = false;
            let datum = pg_sys::SPI_getbinval(htup, tupdesc, i, &mut is_null);

            data.entries
                .entry(i as usize)
                .or_insert_with(|| SpiHeapTupleDataEntry {
                    datum: if is_null { None } else { Some(datum) },
                    type_oid: pg_sys::SPI_gettypeid(tupdesc, i),
                    _table: table,
                });
        }

        data
    }

    /// Get a typed Datum value from this HeapTuple by its ordinal position.
    ///
    /// The ordinal position is 1-based
    #[deprecated(since = "0.1.6", note = "Please use the `by_ordinal` function instead")]
    pub fn get_datum<T: FromDatum>(&self, ordinal: usize) -> Option<T> {
        match self.entries.get(&ordinal) {
            Some(datum) => datum.value(),
            None => None,
        }
    }

    /// Get a typed Datum value from this HeapTuple by its ordinal position.
    ///
    /// The ordinal position is 1-based.
    ///
    /// If the specified ordinal is out of bounds a `Err(SpiError::Noattribute)` is returned
    pub fn by_ordinal(
        &self,
        ordinal: usize,
    ) -> std::result::Result<&SpiHeapTupleDataEntry<'t>, SpiError> {
        match self.entries.get(&ordinal) {
            Some(datum) => Ok(datum),
            None => Err(SpiError::Noattribute),
        }
    }

    /// Get a typed Datum value from this HeapTuple by its field name.
    ///
    /// If the specified name does not exist a `Err(SpiError::Noattribute)` is returned
    pub fn by_name(&self, name: &str) -> std::result::Result<&SpiHeapTupleDataEntry<'t>, SpiError> {
        use crate::pg_sys::AsPgCStr;
        unsafe {
            let fnumber = pg_sys::SPI_fnumber(self.tupdesc, name.as_pg_cstr());
            if fnumber == pg_sys::SPI_ERROR_NOATTRIBUTE {
                Err(SpiError::Noattribute)
            } else {
                self.by_ordinal(fnumber as usize)
            }
        }
    }

    /// Get a mutable typed Datum value from this HeapTuple by its ordinal position.
    ///
    /// The ordinal position is 1-based.
    ///
    /// If the specified ordinal is out of bounds a `Err(SpiError::Noattribute)` is returned
    pub fn by_ordinal_mut(
        &mut self,
        ordinal: usize,
    ) -> std::result::Result<&mut SpiHeapTupleDataEntry<'t>, SpiError> {
        match self.entries.get_mut(&ordinal) {
            Some(datum) => Ok(datum),
            None => Err(SpiError::Noattribute),
        }
    }

    /// Get a mutable typed Datum value from this HeapTuple by its field name.
    ///
    /// If the specified name does not exist a `Err(SpiError::Noattribute)` is returned
    pub fn by_name_mut(
        &mut self,
        name: &str,
    ) -> std::result::Result<&mut SpiHeapTupleDataEntry<'t>, SpiError> {
        use crate::pg_sys::AsPgCStr;
        unsafe {
            let fnumber = pg_sys::SPI_fnumber(self.tupdesc, name.as_pg_cstr());
            if fnumber == pg_sys::SPI_ERROR_NOATTRIBUTE {
                Err(SpiError::Noattribute)
            } else {
                self.by_ordinal_mut(fnumber as usize)
            }
        }
    }

    /// Set a datum value for the specified ordinal position
    ///
    /// If the specified ordinal is out of bounds a `Err(SpiError::Noattribute)` is returned
    pub fn set_by_ordinal<T: IntoDatum + FromDatum>(
        &mut self,
        ordinal: usize,
        datum: T,
    ) -> std::result::Result<(), SpiError> {
        unsafe {
            if ordinal < 1 || ordinal > self.tupdesc.as_ref().unwrap().natts as usize {
                Err(SpiError::Noattribute)
            } else {
                self.entries.insert(
                    ordinal,
                    SpiHeapTupleDataEntry {
                        datum: datum.into_datum(),
                        type_oid: T::type_oid(),
                        _table: self._table,
                    },
                );
                Ok(())
            }
        }
    }

    /// Set a datum value for the specified field name
    ///
    /// If the specified name does not exist a `Err(SpiError::Noattribute)` is returned
    pub fn set_by_name<T: IntoDatum + FromDatum>(
        &mut self,
        name: &str,
        datum: T,
    ) -> std::result::Result<(), SpiError> {
        use crate::pg_sys::AsPgCStr;
        unsafe {
            let fnumber = pg_sys::SPI_fnumber(self.tupdesc, name.as_pg_cstr());
            if fnumber == pg_sys::SPI_ERROR_NOATTRIBUTE {
                Err(SpiError::Noattribute)
            } else {
                self.set_by_ordinal(fnumber as usize, datum)
            }
        }
    }
}
// FIXME
// impl<Datum: IntoDatum + FromDatum> From<Datum> for SpiHeapTupleDataEntry<'_> {
//     fn from(datum: Datum) -> Self {
//         SpiHeapTupleDataEntry {
//             datum: datum.into_datum(),
//             type_oid: Datum::type_oid(),
//             table:
//         }
//     }
// }

impl SpiHeapTupleDataEntry<'_> {
    pub fn value<T: FromDatum>(&self) -> Option<T> {
        match self.datum.as_ref() {
            Some(datum) => unsafe { T::from_datum(*datum, false, self.type_oid) },
            None => None,
        }
    }
}

/// Provide ordinal indexing into a `SpiHeapTupleData`.
///
/// If the index is out of bounds, it will panic
impl<'td> Index<usize> for SpiHeapTupleData<'td> {
    type Output = SpiHeapTupleDataEntry<'td>;

    fn index(&self, index: usize) -> &Self::Output {
        self.by_ordinal(index).expect("invalid ordinal value")
    }
}

/// Provide named indexing into a `SpiHeapTupleData`.
///
/// If the field name doesn't exist, it will panic
impl<'t> Index<&str> for SpiHeapTupleData<'t> {
    type Output = SpiHeapTupleDataEntry<'t>;

    fn index(&self, index: &str) -> &Self::Output {
        self.by_name(index).expect("invalid field name")
    }
}

/// Provide mutable ordinal indexing into a `SpiHeapTupleData`.
///
/// If the index is out of bounds, it will panic
impl<'t> IndexMut<usize> for SpiHeapTupleData<'t> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.by_ordinal_mut(index).expect("invalid ordinal value")
    }
}

/// Provide mutable named indexing into a `SpiHeapTupleData`.
///
/// If the field name doesn't exist, it will panic
impl IndexMut<&str> for SpiHeapTupleData<'_> {
    fn index_mut(&mut self, index: &str) -> &mut Self::Output {
        self.by_name_mut(index).expect("invalid field name")
    }
}

pub struct SpiTupleTableIter<'t> {
    table: &'t SpiTupleTable<'t>,
    current: usize,
}

impl<'t> Iterator for SpiTupleTableIter<'t> {
    type Item = SpiHeapTupleData<'t>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.table.size {
            None
        } else {
            let res = self.table.get_heap_tuple();
            self.current += 1;
            res
        }
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.table.size))
    }

    #[inline]
    fn count(self) -> usize
        where
            Self: Sized,
    {
        self.table.size
    }
}