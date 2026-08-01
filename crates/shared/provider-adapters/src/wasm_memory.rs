use std::net::IpAddr;

use wasmtime::{Instance, Memory, Store, TypedFunc};

use super::wasm::WasmStoreState;

pub(super) fn public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [first, second, third, _] = address.octets();
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast()
                || first == 0
                || (first == 100 && (64..=127).contains(&second))
                || (first == 192 && second == 0 && third == 0)
                || (first == 192 && second == 88 && third == 99)
                || (first == 198 && (18..=19).contains(&second))
                || first >= 240)
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            (0x2000..=0x3fff).contains(&segments[0])
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
                && !address.is_multicast()
        }
    }
}

pub(super) fn typed<Params, Results>(
    instance: &Instance,
    store: &mut Store<WasmStoreState>,
    name: &str,
) -> Result<TypedFunc<Params, Results>, String>
where
    Params: wasmtime::WasmParams,
    Results: wasmtime::WasmResults,
{
    instance
        .get_typed_func(store, name)
        .map_err(|error| error.to_string())
}

pub(super) fn write_memory(
    memory: &Memory,
    store: &mut Store<WasmStoreState>,
    offset: usize,
    bytes: &[u8],
) -> Result<(), String> {
    memory
        .write(store, offset, bytes)
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn read_memory(
    memory: &Memory,
    store: &mut Store<WasmStoreState>,
    offset: usize,
    len: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0; len];
    memory
        .read(store, offset, &mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
