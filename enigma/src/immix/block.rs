//use std::alloc::{Alloc, Global, Layout};
use allocator_api::{Alloc, Global, Layout};
use std::cell::Cell;
use std::cmp;
use std::mem;
use std::ptr::{self, NonNull};

// TODO: implement immix lines later on

/// The number of bytes in a block (+ the header).
pub const DEFAULT_BLOCK_SIZE: usize = 32 * 1024 + mem::size_of::<Block>();

pub const DEFAULT_BLOCK_ALIGN: usize = mem::align_of::<Block>();

pub struct Block {
    /// Points to the start of the block (including this header)
    data: NonNull<u8>,

    /// Block layout (total size)
    layout: Layout,

    /// Link to the next block, if any.
    next: Cell<Option<NonNull<Block>>>,

    /// Cursor to the current free spot when bump allocating.
    ptr: Cell<NonNull<u8>>,
}

// -- block
// metadata (Block)
// oo memory space <-- Block.data
// oo memory space
// oo memory space <-- Block.end
// -- end

#[derive(Debug)]
pub struct Heap {
    // The current block we are bump allocating within.
    current_block: Cell<NonNull<Block>>,

    // The first block we were ever given, which is the head of the intrusive
    // linked list of all blocks this arena has been bump allocating within.
    all_blocks: Cell<NonNull<Block>>,
}

unsafe impl Sync for Heap {}
unsafe impl Send for Heap {}

#[inline]
pub(crate) fn round_up_to(n: usize, divisor: usize) -> usize {
    debug_assert!(divisor.is_power_of_two());
    (n + divisor - 1) & !(divisor - 1)
}

impl Block {
    fn default_block_layout() -> Layout {
        if cfg!(debug_assertions) {
            Layout::from_size_align(DEFAULT_BLOCK_SIZE, DEFAULT_BLOCK_ALIGN).unwrap()
        } else {
            unsafe { Layout::from_size_align_unchecked(DEFAULT_BLOCK_SIZE, DEFAULT_BLOCK_ALIGN) }
        }
    }

    /// Allocate a new block and return its initialized header.
    ///
    /// If given, `alloc_layout` is the layout of the allocation request that
    /// triggered us to fall back to allocating a new block of memory.
    #[allow(clippy::cast_ptr_alignment)]
    fn new(alloc_layout: Option<Layout>) -> NonNull<Block> {
        let layout = alloc_layout.map_or_else(Block::default_block_layout, |l| {
            let align = cmp::max(l.align(), mem::align_of::<Block>());
            if l.size() < DEFAULT_BLOCK_SIZE {
                // If it is a small allocation, just use our default block size,
                // but make sure it is aligned for the requested allocation.
                Layout::from_size_align(DEFAULT_BLOCK_SIZE, align).unwrap()
            } else {
                // If the requested allocation is bigger than we can fit in one
                // of our default blocks, make a special block just for this
                // allocation.
                //
                // Round the size up to a multiple of our header's alignment so
                // that we can be sure that our header is properly aligned.
                let size = round_up_to(l.size(), mem::align_of::<Block>());
                Layout::from_size_align(size + mem::size_of::<Block>(), align).unwrap()
            }
        });

        let size = layout.size();

        unsafe {
            let data = Global.alloc(layout).unwrap();

            let next = Cell::new(None);
            let ptr = Cell::new(data);
            let footer_ptr = data.as_ptr() as usize + size - mem::size_of::<Block>();
            let footer_ptr = footer_ptr as *mut Block;

            ptr::write(
                footer_ptr,
                Block {
                    data,
                    layout,
                    next,
                    ptr,
                },
            );
            NonNull::new_unchecked(footer_ptr)
        }
    }
}

impl Heap {
    pub fn new() -> Self {
        let block = Block::new(None);
        Heap {
            current_block: Cell::new(block),
            all_blocks: Cell::new(block),
        }
    }

    /// Allocate an object.
    ///
    /// ## Example
    ///
    /// ```
    /// let heap = Heap::new();
    /// let x = heap.alloc("hello");
    /// assert_eq!(*x, "hello");
    /// ```
    #[inline(always)]
    pub fn alloc<T>(&self, val: T) -> &mut T {
        let layout = Layout::new::<T>();

        unsafe {
            let p = self.alloc_layout(layout);
            let p = p.as_ptr() as *mut T;
            ptr::write(p, val);
            &mut *p
        }
    }

    #[inline(always)]
    pub fn alloc_layout(&self, layout: Layout) -> NonNull<u8> {
        unsafe {
            let footer = self.current_block.get();
            let footer = footer.as_ref();
            let ptr = footer.ptr.get().as_ptr() as usize;
            let ptr = round_up_to(ptr, layout.align());
            let end = footer as *const _ as usize;
            debug_assert!(ptr <= end);

            let new_ptr = match ptr.checked_add(layout.size()) {
                Some(p) => p,
                None => self.overflow(),
            };

            if new_ptr <= end {
                let p = ptr as *mut u8;
                debug_assert!(new_ptr <= footer as *const _ as usize);
                footer.ptr.set(NonNull::new_unchecked(new_ptr as *mut u8));
                return NonNull::new_unchecked(p);
            }
        }

        // TODO: verify new block alloc works
        self.alloc_layout_slow(layout)
    }

    #[inline(never)]
    #[cold]
    fn overflow(&self) -> ! {
        panic!("allocation too large, caused overflow")
    }

    // Slow path allocation for when we need to allocate a new block from the
    // parent bump set because there isn't enough room in our current block.
    #[inline(never)]
    fn alloc_layout_slow(&self, layout: Layout) -> NonNull<u8> {
        unsafe {
            // Get a new block from the global allocator.
            let size = layout.size();
            let footer = Block::new(Some(layout));

            // Set our current block's next link to this new block.
            self.current_block.get().as_ref().next.set(Some(footer));

            // Set the new block as our new current block.
            self.current_block.set(footer);

            // Move the bump ptr finger ahead to allocate room for `val`.
            let footer = footer.as_ref();
            let ptr = footer.ptr.get().as_ptr() as usize + size;
            debug_assert!(
                ptr <= footer as *const _ as usize,
                "{} <= {}",
                ptr,
                footer as *const _ as usize
            );
            footer.ptr.set(NonNull::new_unchecked(ptr as *mut u8));

            // Return a pointer to the start of this block.
            footer.data.cast::<u8>()
        }
    }

    // pub fn dealloc_block(ptr: BlockPtr, size: BlockSize) {
    //     unsafe {
    //         let layout = Layout::from_size_align_unchecked(size, size);

    //         Global.dealloc(ptr, layout);
    //     }
    // }
}
