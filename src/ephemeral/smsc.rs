use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicUsize, Ordering},
};

const N: usize = 16; // arena size

/// Preallocates memory and attempts to increase
/// consume/produce efficiency by using an arena
pub struct EphemeralBuffer<T> {
    ring: UnsafeCell<[MaybeUninit<T>; N]>,
    head: AtomicUsize, // read index
    tail: AtomicUsize, // write index
}

impl<T> EphemeralBuffer<T> {
    pub const fn new() -> Self {
        Self {
            ring: unsafe { MaybeUninit::uninit().assume_init() },
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
}

pub fn sink_value<T>(b: &EphemeralBuffer<T>, val: T) -> Result<(), T> {
    let head = b.head.load(Ordering::Acquire);
    let tail = b.tail.load(Ordering::Relaxed);
    let next = (tail + 1) % N;

    // guard: empty or full
    if next == head {
        return Err(val);
    }

    unsafe { (*b.ring.get())[tail].as_mut_ptr().write(val) };
    b.tail.store(next, Ordering::Release);

    Ok(())
}

pub fn spit_value<T>(b: &EphemeralBuffer<T>) -> Option<T> {
    let head = b.head.load(Ordering::Acquire);
    let tail = b.tail.load(Ordering::Relaxed);
    let next = (head + 1) % N;

    // guard: empty
    if head == tail {
        return None;
    }

    let val = unsafe { (*b.ring.get())[head].as_ptr().read() };
    b.head.store(next, Ordering::Release);
    Some(val)
}

unsafe impl<T> Sync for EphemeralBuffer<T> {}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_seq_spsc() {
        let src = EphemeralBuffer::<i32>::new();

        for i in 0..10000 {
            if sink_value(&src, i).is_ok() {
                let tmp = spit_value(&src).expect("Failed to produce");
                assert_eq!(tmp, i);
                continue;
            }
            panic!("Failed to consume {}", i);
        }
    }

    #[test]
    fn test_threaded_spsc() {
        let src = Arc::new(EphemeralBuffer::<i32>::new());

        let producer = src.clone();
        let produce_t = thread::spawn(move || {
            for i in 0..10000 {
                while sink_value(&producer, i).is_err() {
                    thread::yield_now();
                }
            }
        });

        let consumer = src.clone();
        let consume_t = thread::spawn(move || {
            for i in 0..10000 {
                loop {
                    if let Some(result) = spit_value(&consumer) {
                        assert_eq!(result, i);
                        break;
                    }
                    thread::yield_now();
                }
            }
        });

        produce_t.join().unwrap();
        consume_t.join().unwrap();
    }
}
