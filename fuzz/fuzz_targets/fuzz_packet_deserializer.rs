#![no_main]

use libfuzzer_sys::fuzz_target;
use omnimesh_sdk::WirePacket;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize data as a WirePacket
    let _ = bincode::deserialize::<WirePacket>(data);
});
