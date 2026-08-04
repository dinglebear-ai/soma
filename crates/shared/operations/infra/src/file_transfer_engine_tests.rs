use super::*;

#[test]
fn file_transfer_engine_is_zero_sized() {
    assert_eq!(std::mem::size_of::<FileTransferEngine>(), 0);
}
