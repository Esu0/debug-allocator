#![feature(allocator_api)]

use std::alloc::Global;

use debug_allocator::alloc::DebugAlloc;

fn main() {
    let allocator = DebugAlloc::new(Global);
    let mut v = Vec::new_in(allocator.clone());
    for i in (0..10000).map(|i| (i % 256) as u8) {
        v.push(i);
    }

    v.truncate(10);
    let _ = v.into_boxed_slice();

    allocator.dump_n(5);
}