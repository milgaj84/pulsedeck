#[cfg(target_os = "windows")]
pub(super) fn get_user_idle_ms() -> Option<u64> {
    #[repr(C)]
    #[allow(clippy::upper_case_acronyms)] // Mirrors the Win32 API struct name.
    struct LASTINPUTINFO {
        cb_size: u32,
        dw_time: u32,
    }

    extern "system" {
        fn GetLastInputInfo(plii: *mut LASTINPUTINFO) -> i32;
        fn GetTickCount64() -> u64;
    }

    let mut lii = LASTINPUTINFO {
        cb_size: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dw_time: 0,
    };

    unsafe {
        if GetLastInputInfo(&mut lii) != 0 {
            let tick = GetTickCount64();
            // LASTINPUTINFO.dwTime is still u32, but GetTickCount64 is u64.
            // We compare only the lower 32 bits for the delta.
            let last_input_64 = lii.dw_time as u64;
            let tick_low = tick & 0xFFFF_FFFF;
            let idle = if tick_low >= last_input_64 {
                tick_low - last_input_64
            } else {
                // u32 rollover: last_input was near u32::MAX, tick_low wrapped.
                (0x1_0000_0000u64 - last_input_64) + tick_low
            };
            Some(idle)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn get_user_idle_ms() -> Option<u64> {
    None
}
