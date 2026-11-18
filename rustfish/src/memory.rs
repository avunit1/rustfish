use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU64, Ordering};

#[repr(C, align(64))]
pub struct CachePadded<T>(pub T);

impl<T> CachePadded<T> {
    #[inline(always)]
    pub const fn new(v: T) -> Self {
        Self(v)
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for CachePadded<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Single-writer / multi-reader relaxed atomic counter.
/// Increments use relaxed load + add + store (emitting a plain non-locked ADD/INC on x86-64,
/// matching C++ Stockfish's `RelaxedAtomic<u64>` and avoiding hardware `LOCK XADD`).
#[repr(transparent)]
pub struct RelaxedAtomicU64(AtomicU64);

impl RelaxedAtomicU64 {
    #[inline(always)]
    pub const fn new(v: u64) -> Self {
        Self(AtomicU64::new(v))
    }

    #[inline(always)]
    pub fn load(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub fn store(&self, v: u64) {
        self.0.store(v, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn inc(&self) -> u64 {
        let v = self.0.load(Ordering::Relaxed).wrapping_add(1);
        self.0.store(v, Ordering::Relaxed);
        v
    }
}

/// Allocation metadata returned by the OS page allocator so `free_pages` can release it cleanly.
#[derive(Copy, Clone)]
#[allow(dead_code)]
struct AllocMeta {
    size: usize,
    align: usize,
}

#[cfg(windows)]
mod os_alloc {
    use super::AllocMeta;
    use core::ffi::c_void;

    const MEM_COMMIT: u32 = 0x0000_1000;
    const MEM_RESERVE: u32 = 0x0000_2000;
    const MEM_RELEASE: u32 = 0x0000_8000;
    const MEM_LARGE_PAGES: u32 = 0x2000_0000;
    const PAGE_READWRITE: u32 = 0x04;

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
    const SE_PRIVILEGE_ENABLED: u32 = 0x0000_0002;
    const ERROR_SUCCESS: u32 = 0;

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct Luid {
        low_part: u32,
        high_part: i32,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct LuidAndAttributes {
        luid: Luid,
        attributes: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct TokenPrivileges {
        privilege_count: u32,
        privileges: [LuidAndAttributes; 1],
    }

    type OpenProcessTokenFn = unsafe extern "system" fn(isize, u32, *mut isize) -> i32;
    type LookupPrivilegeValueAFn = unsafe extern "system" fn(*const u8, *const u8, *mut Luid) -> i32;
    type AdjustTokenPrivilegesFn = unsafe extern "system" fn(
        isize,
        i32,
        *const TokenPrivileges,
        u32,
        *mut TokenPrivileges,
        *mut u32,
    ) -> i32;

    extern "system" {
        fn VirtualAlloc(addr: *mut u8, size: usize, alloc_type: u32, protect: u32) -> *mut u8;
        fn VirtualFree(addr: *mut u8, size: usize, free_type: u32) -> i32;
        fn GetLargePageMinimum() -> usize;
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn GetLastError() -> u32;
        fn GetModuleHandleA(name: *const u8) -> *mut c_void;
        fn LoadLibraryA(name: *const u8) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, proc_name: *const u8) -> *const c_void;
    }

    /// Mirrors C++ Stockfish's `aligned_large_pages_alloc_windows` (`memory.h` / `memory.cpp`):
    /// enables `SeLockMemoryPrivilege` temporarily, rounds size up to `GetLargePageMinimum()`,
    /// and attempts `VirtualAlloc(..., MEM_LARGE_PAGES)`. Returns null on failure.
    unsafe fn try_alloc_large_pages(bytes: usize) -> Option<(*mut u8, AllocMeta)> {
        let large_page_size = GetLargePageMinimum();
        if large_page_size == 0 {
            return None;
        }

        let mut advapi = GetModuleHandleA(b"advapi32.dll\0".as_ptr());
        if advapi.is_null() {
            advapi = LoadLibraryA(b"advapi32.dll\0".as_ptr());
        }
        if advapi.is_null() {
            return None;
        }

        let open_token_p = GetProcAddress(advapi, b"OpenProcessToken\0".as_ptr());
        let lookup_priv_p = GetProcAddress(advapi, b"LookupPrivilegeValueA\0".as_ptr());
        let adjust_priv_p = GetProcAddress(advapi, b"AdjustTokenPrivileges\0".as_ptr());
        if open_token_p.is_null() || lookup_priv_p.is_null() || adjust_priv_p.is_null() {
            return None;
        }

        let open_process_token: OpenProcessTokenFn = core::mem::transmute(open_token_p);
        let lookup_privilege_value: LookupPrivilegeValueAFn = core::mem::transmute(lookup_priv_p);
        let adjust_token_privileges: AdjustTokenPrivilegesFn = core::mem::transmute(adjust_priv_p);

        let mut token: isize = 0;
        if open_process_token(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0 {
            return None;
        }

        let mut luid = Luid::default();
        if lookup_privilege_value(core::ptr::null(), b"SeLockMemoryPrivilege\0".as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return None;
        }

        let tp = TokenPrivileges {
            privilege_count: 1,
            privileges: [LuidAndAttributes {
                luid,
                attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let mut prev_tp = TokenPrivileges::default();
        let mut prev_len: u32 = 0;

        let adjusted = adjust_token_privileges(
            token,
            0,
            &tp,
            core::mem::size_of::<TokenPrivileges>() as u32,
            &mut prev_tp,
            &mut prev_len,
        );
        if adjusted == 0 || GetLastError() != ERROR_SUCCESS {
            CloseHandle(token);
            return None;
        }

        let alloc_size = (bytes.max(large_page_size) + large_page_size - 1) & !(large_page_size - 1);
        let ptr = VirtualAlloc(
            core::ptr::null_mut(),
            alloc_size,
            MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES,
            PAGE_READWRITE,
        );

        // Restore previous token privilege state
        adjust_token_privileges(token, 0, &prev_tp, 0, core::ptr::null_mut(), core::ptr::null_mut());
        CloseHandle(token);

        if ptr.is_null() {
            None
        } else {
            Some((
                ptr,
                AllocMeta {
                    size: alloc_size,
                    align: large_page_size,
                },
            ))
        }
    }

    pub unsafe fn alloc_zeroed_pages(bytes: usize, try_large_pages: bool) -> (*mut u8, AllocMeta) {
        if try_large_pages {
            if let Some(res) = try_alloc_large_pages(bytes) {
                return res;
            }
        }
        let size = bytes.max(4096);
        let ptr = VirtualAlloc(core::ptr::null_mut(), size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
        assert!(!ptr.is_null(), "VirtualAlloc failed for {} bytes", size);
        (ptr, AllocMeta { size, align: 4096 })
    }

    pub unsafe fn free_pages(ptr: *mut u8, _meta: AllocMeta) {
        if !ptr.is_null() {
            let ok = VirtualFree(ptr, 0, MEM_RELEASE);
            debug_assert!(ok != 0);
        }
    }
}

#[cfg(not(windows))]
mod os_alloc {
    use super::AllocMeta;

    #[cfg(target_os = "linux")]
    const HUGE_PAGE_ALIGN: usize = 2 * 1024 * 1024;
    #[cfg(target_os = "linux")]
    const MADV_HUGEPAGE: core::ffi::c_int = 14;

    #[cfg(target_os = "linux")]
    extern "C" {
        fn madvise(
            addr: *mut core::ffi::c_void,
            length: usize,
            advice: core::ffi::c_int,
        ) -> core::ffi::c_int;
    }

    pub unsafe fn alloc_zeroed_pages(bytes: usize, try_large_pages: bool) -> (*mut u8, AllocMeta) {
        #[cfg(target_os = "linux")]
        let align = if try_large_pages && bytes >= HUGE_PAGE_ALIGN {
            HUGE_PAGE_ALIGN
        } else {
            4096
        };
        #[cfg(not(target_os = "linux"))]
        let align = {
            let _ = try_large_pages;
            4096
        };

        let size = (bytes.max(align) + align - 1) & !(align - 1);
        let layout = std::alloc::Layout::from_size_align(size, align).expect("invalid page layout");
        let ptr = std::alloc::alloc_zeroed(layout);
        assert!(!ptr.is_null(), "alloc_zeroed failed for {} bytes (align {})", size, align);

        #[cfg(target_os = "linux")]
        if try_large_pages && align == HUGE_PAGE_ALIGN {
            // Best-effort transparent huge page hint; failure is non-fatal.
            let _ = madvise(ptr as *mut core::ffi::c_void, size, MADV_HUGEPAGE);
        }

        (ptr, AllocMeta { size, align })
    }

    pub unsafe fn free_pages(ptr: *mut u8, meta: AllocMeta) {
        if !ptr.is_null() {
            let layout = std::alloc::Layout::from_size_align(meta.size, meta.align)
                .expect("invalid page layout on free");
            std::alloc::dealloc(ptr, layout);
        }
    }
}

/// Page-aligned zero-initialized heap slice.
/// Uses 2MB large pages (`MEM_LARGE_PAGES` on Windows, `MADV_HUGEPAGE` on Linux) when requested,
/// falling back cleanly to 4KB page-aligned memory.
pub struct PageAlignedSlice<T> {
    ptr: *mut T,
    len: usize,
    meta: AllocMeta,
    _marker: PhantomData<T>,
}

unsafe impl<T: Send> Send for PageAlignedSlice<T> {}
unsafe impl<T: Sync> Sync for PageAlignedSlice<T> {}

impl<T> PageAlignedSlice<T> {
    pub fn new_zeroed(len: usize) -> Self {
        Self::alloc_inner(len, false)
    }

    /// Allocates a page-aligned zero-initialized slice, attempting 2MB large/huge pages first.
    pub fn new_large_pages_zeroed(len: usize) -> Self {
        Self::alloc_inner(len, true)
    }

    fn alloc_inner(len: usize, try_large_pages: bool) -> Self {
        assert!(core::mem::align_of::<T>() <= 4096);
        let bytes = len
            .checked_mul(core::mem::size_of::<T>())
            .expect("PageAlignedSlice byte size overflow");
        let (raw, meta) = unsafe { os_alloc::alloc_zeroed_pages(bytes, try_large_pages) };
        Self {
            ptr: raw as *mut T,
            len,
            meta,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    #[inline(always)]
    pub fn as_mut_ptr(&self) -> *mut T {
        self.ptr
    }

    #[inline(always)]
    pub unsafe fn get_unchecked(&self, idx: usize) -> &T {
        &*self.ptr.add(idx)
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl<T> Drop for PageAlignedSlice<T> {
    fn drop(&mut self) {
        unsafe {
            if core::mem::needs_drop::<T>() {
                for i in 0..self.len {
                    core::ptr::drop_in_place(self.ptr.add(i));
                }
            }
            os_alloc::free_pages(self.ptr as *mut u8, self.meta);
        }
    }
}

/// Page-aligned single object heap allocation (matching C++ Stockfish's `LargePagePtr<T>`).
pub struct PageAlignedBox<T> {
    ptr: *mut T,
    meta: AllocMeta,
    _marker: PhantomData<T>,
}

unsafe impl<T: Send> Send for PageAlignedBox<T> {}
unsafe impl<T: Sync> Sync for PageAlignedBox<T> {}

impl<T> PageAlignedBox<T> {
    pub fn new(val: T) -> Self {
        assert!(core::mem::align_of::<T>() <= 4096);
        let bytes = core::mem::size_of::<T>();
        let (raw, meta) = unsafe { os_alloc::alloc_zeroed_pages(bytes, false) };
        let ptr = raw as *mut T;
        unsafe {
            ptr.write(val);
        }
        Self {
            ptr,
            meta,
            _marker: PhantomData,
        }
    }

    pub fn new_zeroed() -> Self {
        Self::alloc_zeroed_inner(false)
    }

    /// Allocates a page-aligned zero-initialized box, attempting 2MB large/huge pages first.
    pub fn new_large_pages_zeroed() -> Self {
        Self::alloc_zeroed_inner(true)
    }

    fn alloc_zeroed_inner(try_large_pages: bool) -> Self {
        assert!(core::mem::align_of::<T>() <= 4096);
        let bytes = core::mem::size_of::<T>();
        let (raw, meta) = unsafe { os_alloc::alloc_zeroed_pages(bytes, try_large_pages) };
        Self {
            ptr: raw as *mut T,
            meta,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    #[inline(always)]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }
}

impl<T> Deref for PageAlignedBox<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for PageAlignedBox<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }
}

impl<T> Drop for PageAlignedBox<T> {
    fn drop(&mut self) {
        unsafe {
            core::ptr::drop_in_place(self.ptr);
            os_alloc::free_pages(self.ptr as *mut u8, self.meta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_aligned_and_large_page_fallback() {
        // 4KB page-aligned slice
        let s: PageAlignedSlice<u64> = PageAlignedSlice::new_zeroed(1024);
        assert_eq!(s.as_ptr() as usize % 4096, 0);
        assert_eq!(s.as_slice()[0], 0);
        assert_eq!(s.as_slice()[1023], 0);

        // 4MB buffer requesting 2MB large pages (uses MEM_LARGE_PAGES / MADV_HUGEPAGE if available,
        // otherwise falls back cleanly to 4KB aligned pages).
        let big: PageAlignedSlice<u64> = PageAlignedSlice::new_large_pages_zeroed(512 * 1024);
        assert_eq!(big.as_ptr() as usize % 4096, 0);
        assert_eq!(big.as_slice()[0], 0);
        assert_eq!(big.as_slice()[512 * 1024 - 1], 0);

        // Large-page box
        let b: PageAlignedBox<[u64; 1024]> = PageAlignedBox::new_large_pages_zeroed();
        assert_eq!(b.as_ptr() as usize % 4096, 0);
        assert_eq!(b[0], 0);
    }
}
