use std::mem;
use std::ptr;
use std::ops::Deref;
use std::borrow::{Borrow, BorrowMut};

use libc;

pub struct CPtr<T: Send>(*mut T);

impl<T: Send> CPtr<T> {
    pub fn new(value: T) -> CPtr<T> {
        unsafe {
            let ptr = libc::malloc(mem::size_of::<T>() as libc::size_t) as *mut T;

            // we *need* valid pointer.
            assert!(!ptr.is_null());

            // `*ptr` is uninitialized, and `*ptr = value` would
            // attempt to destroy it `overwrite` moves a value into
            // this memory without attempting to drop the original
            // value.
            ptr::write(&mut *ptr, value);

            CPtr(ptr)
        }
    }

    #[inline]
    pub fn from_ptr(p: *mut T) -> CPtr<T> {
        CPtr(p)
    }
}

impl<T: Send> Borrow<T> for CPtr<T> {
    // the 'r lifetime results in the same semantics as `&*x` with Box<T>
    #[inline]
    fn borrow<'r>(&'r self) -> &'r T {
        // By construction, self.ptr is valid
        unsafe { &*self.0 }
    }
}

impl<T: Send> BorrowMut<T> for CPtr<T> {
    // the 'r lifetime results in the same semantics as `&*x` with Box<T>
    #[inline]
    fn borrow_mut<'r>(&'r mut self) -> &'r mut T {
        // By construction, self.ptr is valid
        unsafe { &mut *self.0 }
    }
}

impl<T: Send> Drop for CPtr<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            // Copy the object out from the pointer onto the stack,
            // where it is covered by normal Rust destructor semantics
            // and cleans itself up, if necessary
            ptr::read(self.0 as *const T);

            // clean-up our allocation
            libc::free(self.0 as *mut libc::c_void);

            self.0 = ptr::null_mut();
        }
    }
}

impl<T: Send> Deref for CPtr<T> {
    type Target = *mut T;

    #[inline]
    fn deref(&self) -> &*mut T {
        &self.0
    }
}

#[cfg(test)]
pub mod tests {
    use std::ptr;
    use std::mem;
    use std::borrow::Borrow;
    use libc;

    use super::*;

    struct Foo {
        bar: u32,
    }

    fn validate_borrow<T: Borrow<Foo>>(b: T) {
        assert_eq!(b.borrow().bar, 32);
    }


    #[test]
    fn test_borrow() {
        let p = CPtr::<Foo>::new(Foo { bar: 32 });

        assert!(*p != ptr::null_mut());

        validate_borrow(p);
    }

    #[test]
    fn test_from_ptr() {
        unsafe {
            let foo = libc::malloc(mem::size_of::<Foo>() as libc::size_t) as *mut Foo;

            (*foo).bar = 32;

            let p = CPtr::<Foo>::from_ptr(foo);

            assert!(*p != ptr::null_mut());
            assert_eq!((**p).bar, 32);
        }
    }
}
