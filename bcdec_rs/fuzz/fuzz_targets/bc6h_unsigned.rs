#![no_main]

extern crate libfuzzer_sys;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: [u8; 16]| {
    // 4x4 RGB f16
    // The pitch is in terms of half floats rather than bytes.
    let mut expected = [0u8; 4 * 4 * 6];
    unsafe {
        bcndecode_sys::bcdec_bc6h_half(data.as_ptr(), expected.as_mut_ptr() as _, 4 * 3, 0);
    }

    let mut actual = [0u8; 4 * 4 * 6];
    bcdec_rs::bc6h_half(&data, &mut actual, 4 * 3, false);

    assert_eq!(expected, actual);
});
