use std::{
    alloc::{AllocError, Allocator, Layout},
    collections::VecDeque,
    fmt::{self, Debug, Display},
    ptr::NonNull,
    sync::{Arc, RwLock, RwLockReadGuard},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Action {
    pub addr: Option<NonNull<()>>,
    pub layout: Layout,
    pub kind: Kind,
}

unsafe impl Send for Action {}
unsafe impl Sync for Action {}

fn fmt_layout(f: &mut fmt::Formatter<'_>, layout: Layout) -> fmt::Result {
    write!(
        f,
        "{{ size: {}, align: {} }}",
        layout.size(),
        layout.align()
    )
}

impl Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            Kind::Allocate => write!(f, "allocate"),
            Kind::Deallocate => write!(f, "deallocate"),
            Kind::AllocateZeroed => write!(f, "allocate_zeroed"),
            Kind::Grow(layout) => {
                write!(f, "grow\n\told_layout: ")?;
                fmt_layout(f, layout)
            }
            Kind::GrowZeroed(layout) => {
                write!(f, "grow_zeroed\n\told_layout: ")?;
                fmt_layout(f, layout)
            }
            Kind::Shrink(layout) => {
                write!(f, "shrink\n\told_layout: ")?;
                fmt_layout(f, layout)
            }
        }?;
        match self.kind {
            Kind::Allocate | Kind::AllocateZeroed | Kind::Deallocate => {
                write!(f, "\n\tlayout: ")
            }
            Kind::Grow(_) | Kind::GrowZeroed(_) | Kind::Shrink(_) => {
                write!(f, "\n\tnew_layout: ")
            }
        }?;
        fmt_layout(f, self.layout)?;
        if let Some(addr) = self.addr {
            writeln!(f, "\n\taddress: {:p}", addr)
        } else {
            writeln!(f, "\n\taddress: Allocation Error")
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Allocate,
    Deallocate,
    AllocateZeroed,
    Grow(Layout),
    GrowZeroed(Layout),
    Shrink(Layout),
}

#[derive(Clone, Debug)]
pub struct DebugAlloc<A> {
    alloc: A,
    history: Arc<RwLock<VecDeque<Action>>>,
}

impl<A> DebugAlloc<A> {
    pub fn new(alloc: A) -> Self {
        Self {
            alloc,
            history: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub fn history(&self) -> RwLockReadGuard<'_, VecDeque<Action>> {
        self.history.read().unwrap()
    }

    pub fn poisoned(&self) -> bool {
        self.history.is_poisoned()
    }

    /// 全ての履歴を表示する
    pub fn dump_all_history(&self) {
        let history = self.history();
        for action in history.iter().rev() {
            println!("{action}");
        }
    }

    /// 直近の`n`個の履歴を表示する
    pub fn dump_n(&self, n: usize) {
        let history = self.history();
        for action in history.iter().rev().take(n) {
            println!("{action}");
        }
    }

    /// 履歴をすべて削除する
    pub fn clear_history(&self) {
        self.history.write().unwrap().clear();
    }

    /// 履歴を古いものから`n`個削除する
    pub fn pop_history_n(&self, n: usize) {
        let mut wlock = self.history.write().unwrap();
        if wlock.len() < n {
            wlock.clear();
        } else {
            let new = wlock.split_off(n);
            *wlock = new;
        }
    }

    /// 直近の`n`個の履歴を残してそれ以外を削除する
    pub fn shrink_history(&self, n: usize) {
        let mut wlock = self.history.write().unwrap();
        let len = wlock.len();
        if len > n {
            let new = wlock.split_off(len - n);
            *wlock = new;
        }
    }
}

unsafe impl<A: Allocator> Allocator for DebugAlloc<A> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let result = self.alloc.allocate(layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(
                Action {
                    addr: result.ok().map(|ptr| ptr.cast()),
                    layout,
                    kind: Kind::Allocate,
                }
            );
        }
        result
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.alloc.deallocate(ptr, layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(Action {
                addr: Some(ptr.cast()),
                layout,
                kind: Kind::Deallocate,
            });
        }
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let result = self.alloc.allocate_zeroed(layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(
                Action {
                    addr: result.ok().map(|ptr| ptr.cast()),
                    layout,
                    kind: Kind::AllocateZeroed,
                }
            );
        }
        result
    }

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let result = self.alloc.grow(ptr, old_layout, new_layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(Action {
                addr: result.ok().map(|ptr| ptr.cast()),
                layout: new_layout,
                kind: Kind::Grow(old_layout),
            }
            );
        }
        result
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let result = self.alloc.grow_zeroed(ptr, old_layout, new_layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(
                Action {
                    addr: result.ok().map(|ptr| ptr.cast()),
                    layout: new_layout,
                    kind: Kind::GrowZeroed(old_layout),
                }
            );
        }
        result
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        let result = self.alloc.shrink(ptr, old_layout, new_layout);
        if let Ok(mut wlock) = self.history.write() {
            wlock.push_back(
                Action {
                    addr: result.ok().map(|ptr| ptr.cast()),
                    layout: new_layout,
                    kind: Kind::Shrink(old_layout),
                }
            );
        }
        result
    }
}
