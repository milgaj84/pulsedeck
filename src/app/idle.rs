#[cfg(target_os = "windows")]
pub(super) fn get_user_idle_ms() -> Option<u64> {
    // In test builds, never report real idle time — tests don't have user input.
    #[cfg(test)]
    {
        None
    }

    #[cfg(not(test))]
    {
        get_user_idle_ms_impl()
    }
}

#[cfg(target_os = "windows")]
#[cfg(not(test))]
fn get_user_idle_ms_impl() -> Option<u64> {
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
            let last_input_64 = lii.dw_time as u64;
            let tick_low = tick & 0xFFFF_FFFF;
            let idle = if tick_low >= last_input_64 {
                tick_low - last_input_64
            } else {
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
