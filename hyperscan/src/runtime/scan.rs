use core::mem;

use failure::Error;
use foreign_types::{ForeignType, ForeignTypeRef};
use libc::c_uint;

use crate::common::{Block, DatabaseRef, Vectored};
use crate::errors::AsResult;
use crate::ffi;
use crate::runtime::{ScratchRef, Stream};

/// Scannable buffer
pub trait Scannable: AsRef<[u8]> {}

impl<T> Scannable for T where T: AsRef<[u8]> {}

/// Definition of the match event callback function type.
///
/// This callback function will be invoked whenever a match is located in the
/// target data during the execution of a scan. The details of the match are
/// passed in as parameters to the callback function, and the callback function
/// should return a value indicating whether or not matching should continue on
/// the target data. If no callbacks are desired from a scan call, NULL may be
/// provided in order to suppress match production.
///
/// This callback function should not attempt to call Hyperscan API functions on
/// the same stream nor should it attempt to reuse the scratch space allocated
/// for the API calls that caused it to be triggered. Making another call to the
/// Hyperscan library with completely independent parameters should work (for
/// example, scanning a different database in a new stream and with new scratch
/// space), but reusing data structures like stream state and/or scratch space
/// will produce undefined behavior.
///
/// Fn(id: u32, from: u64, to: u64, flags: u32) -> bool
///
pub type MatchEventCallback<D> = Option<fn(id: u32, from: u64, to: u64, flags: u32, data: &D) -> u32>;

impl DatabaseRef<Block> {
    /// pattern matching takes place for block-mode pattern databases.
    pub fn scan<T, D>(
        &self,
        data: T,
        scratch: &ScratchRef,
        callback: MatchEventCallback<D>,
        context: Option<&D>,
    ) -> Result<(), Error>
    where
        T: Scannable,
    {
        let data = data.as_ref();

        unsafe {
            ffi::hs_scan(
                self.as_ptr(),
                data.as_ptr() as *const i8,
                data.len() as u32,
                0,
                scratch.as_ptr(),
                mem::transmute(callback),
                mem::transmute(context),
            )
            .ok()
        }
    }
}

impl DatabaseRef<Vectored> {
    /// pattern matching takes place for vectoring-mode pattern databases.
    pub fn scan<I, T, D>(
        &self,
        data: I,
        scratch: &ScratchRef,
        callback: MatchEventCallback<D>,
        context: Option<&D>,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = T>,
        T: Scannable,
    {
        let (ptrs, lens): (Vec<_>, Vec<_>) = data
            .into_iter()
            .map(|buf| {
                let buf = buf.as_ref();

                (buf.as_ptr() as *const i8, buf.len() as c_uint)
            })
            .unzip();

        unsafe {
            ffi::hs_scan_vector(
                self.as_ptr(),
                ptrs.as_slice().as_ptr() as *const *const i8,
                lens.as_slice().as_ptr() as *const _,
                ptrs.len() as u32,
                0,
                scratch.as_ptr(),
                mem::transmute(callback),
                mem::transmute(context),
            )
            .ok()
        }
    }
}

impl Stream {
    /// pattern matching takes place for stream-mode pattern databases.
    pub fn scan<T, D>(
        &self,
        data: T,
        scratch: &ScratchRef,
        callback: MatchEventCallback<D>,
        context: Option<&D>,
    ) -> Result<(), Error>
    where
        T: Scannable,
    {
        let data = data.as_ref();

        unsafe {
            ffi::hs_scan_stream(
                self.as_ptr(),
                data.as_ptr() as *const i8,
                data.len() as u32,
                0,
                scratch.as_ptr(),
                mem::transmute(callback),
                mem::transmute(context),
            )
            .ok()
        }
    }
}

#[cfg(test)]
pub mod tests {
    use crate::common::*;
    use crate::compile::Builder;
    use crate::errors::HsError;

    #[test]
    fn test_block_scan() {
        let _ = pretty_env_logger::try_init();

        let db: BlockDatabase = pattern! {"test"; CASELESS | SOM_LEFTMOST}.build().unwrap();
        let s = db.alloc().unwrap();

        db.scan::<_, ()>("foo test bar", &s, None, None).unwrap();

        fn callback(id: u32, from: u64, to: u64, flags: u32, _: &BlockDatabase) -> u32 {
            assert_eq!(id, 0);
            assert_eq!(from, 4);
            assert_eq!(to, 8);
            assert_eq!(flags, 0);

            1
        };

        assert_eq!(
            db.scan("foo test bar".as_bytes(), &s, Some(callback), Some(&db))
                .err()
                .unwrap()
                .downcast_ref::<HsError>(),
            Some(&HsError::ScanTerminated)
        );
    }

    #[test]
    fn test_vectored_scan() {
        let _ = pretty_env_logger::try_init();

        let db: VectoredDatabase = pattern! {"test"; CASELESS|SOM_LEFTMOST}.build().unwrap();
        let s = db.alloc().unwrap();

        let data = vec!["foo".as_bytes(), "test".as_bytes(), "bar".as_bytes()];

        db.scan::<_, _, ()>(data, &s, None, None).unwrap();

        fn callback(id: u32, from: u64, to: u64, flags: u32, _: &VectoredDatabase) -> u32 {
            assert_eq!(id, 0);
            assert_eq!(from, 3);
            assert_eq!(to, 7);
            assert_eq!(flags, 0);

            1
        };

        let data = vec!["foo".as_bytes(), "test".as_bytes(), "bar".as_bytes()];

        assert_eq!(
            db.scan(data, &s, Some(callback), Some(&db))
                .err()
                .unwrap()
                .downcast_ref::<HsError>(),
            Some(&HsError::ScanTerminated)
        );
    }

    #[test]
    fn test_streaming_scan() {
        let _ = pretty_env_logger::try_init();

        let db: StreamingDatabase = pattern! {"test"; CASELESS}.build().unwrap();

        let s = db.alloc().unwrap();
        let st = db.open_stream().unwrap();

        let data = vec!["foo", "test", "bar"];

        fn callback(id: u32, from: u64, to: u64, flags: u32, _: &StreamingDatabase) -> u32 {
            assert_eq!(id, 0);
            assert_eq!(from, 0);
            assert_eq!(to, 7);
            assert_eq!(flags, 0);

            0
        }

        for d in data {
            st.scan(d, &s, Some(callback), Some(&db)).unwrap();
        }

        st.close(&s, Some(callback), Some(&db)).unwrap();
    }
}
