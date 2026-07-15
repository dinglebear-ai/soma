pub fn cap_output(value: &[u8], max_bytes: usize) -> String {
    let capped = &value[..value.len().min(max_bytes)];
    String::from_utf8_lossy(capped).to_string()
}
