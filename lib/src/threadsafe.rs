#[allow(unused_imports)]
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, RwLock},
};

#[cfg(not(feature = "threadsafe"))]
pub type Brc<T> = Rc<T>;

#[cfg(not(feature = "threadsafe"))]
pub type BrCell<T> = RefCell<T>;

#[cfg(not(feature = "threadsafe"))]
pub(crate) fn brcell_borrow<T>(cell: &BrCell<T>) -> std::cell::Ref<'_, T> {
    cell.borrow()
}

#[cfg(feature = "threadsafe")]
pub type Brc<T> = Arc<T>;

#[cfg(feature = "threadsafe")]
pub type BrCell<T> = RwLock<T>;

#[cfg(feature = "threadsafe")]
pub(crate) fn brcell_borrow<'a, T>(cell: &'a BrCell<T>) -> std::sync::RwLockReadGuard<'a, T> {
    cell.read().unwrap()
}
