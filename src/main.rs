#![feature(allocator_api)]
use std::alloc::{handle_alloc_error, Allocator, Global, Layout};

use debug_allocator::alloc::{Action, Kind};

fn main() {
    let mut action = Action {
        addr: None,
        layout: Layout::from_size_align(0, 1).unwrap(),
        kind: Kind::Allocate,
    };
    println!("{action}");
    action.kind = Kind::Grow(Layout::from_size_align(16, 4).unwrap());
    action.layout = Layout::from_size_align(32, 4).unwrap();
    println!("{action}");
    let layout = Layout::from_size_align(16, 4).unwrap();
    let ptr = Global.allocate(layout).unwrap_or_else(|_| handle_alloc_error(layout));
    let ptr = unsafe {
        Global.grow(ptr.cast(), layout, action.layout).unwrap_or_else(|_| handle_alloc_error(action.layout))
    };
    action.addr = Some(ptr.cast());
    println!("{action}");
}
