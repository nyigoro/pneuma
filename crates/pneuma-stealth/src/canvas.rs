pub fn deterministic_canvas_noise(seed: &[u8]) -> [u8; 32] {
    let digest = ring::digest::digest(&ring::digest::SHA256, seed);
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}
