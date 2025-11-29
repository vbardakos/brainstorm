use std::cell;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

// Attempt to avoid Mutex
pub struct EphemeralSource<T> {
    value: cell::UnsafeCell<mem::MaybeUninit<T>>,
    packed: AtomicBool,
}

impl<T> EphemeralSource<T> {
    pub const fn new() -> Self {
        Self {
            value: cell::UnsafeCell::new(mem::MaybeUninit::uninit()),
            packed: AtomicBool::new(false),
        }
    }

    // spins until set
    pub fn set(&self, value: T) {
        while self.packed.load(Ordering::Acquire) {
            thread::yield_now();
        }
        unsafe { self._ptr().write(value) };
        self.packed.store(true, Ordering::Release);
    }

    pub fn get(&self) -> Option<T> {
        if !self.packed.load(Ordering::Acquire) {
            return None;
        }
        let value = unsafe { self._ptr().read() };
        self.packed.store(false, Ordering::Release);
        return Some(value);
    }

    unsafe fn _ptr(&self) -> *mut T {
        (*self.value.get()).as_mut_ptr()
    }
}

unsafe impl<T> Sync for EphemeralSource<T> {}

fn main() {
    let source = Arc::new(EphemeralSource::<i32>::new());

    let producer = Arc::clone(&source);
    let produce = thread::spawn(move || {
        for i in 0..1000 {
            producer.set(i);
        }
    });

    let consumer = Arc::clone(&source);
    let consume = thread::spawn(move || {
        for _ in 0..1000 {
            loop {
                if let Some(value) = consumer.get() {
                    println!("just consumed value: {value}");
                    break;
                }
                // value not ready -> spin
                thread::yield_now();
            }
        }
    });

    produce.join().unwrap();
    consume.join().unwrap();
}
