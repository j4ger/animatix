//! Boundary proof for the pinned splitter, runtime, optimizer and JS loader.

#[cfg(target_arch = "wasm32")]
mod probe {
    use wasm_bindgen::prelude::*;

    const fn workload_table() -> [u32; 2048] {
        let mut values = [0u32; 2048];
        let mut state = 0x6d2b_79f5u32;
        let mut index = 0;
        while index < values.len() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            values[index] = state;
            index += 1;
        }
        values
    }

    // The deterministic, high-entropy table makes the capability payload
    // measurable and provides a byte-level proof that it is absent from main.
    static WORKLOAD: [u32; 2048] = workload_table();

    // Keep the actual cold operation inside the imported capability function.
    // Returning a function pointer/backend from the factory can leave its
    // implementation reachable from the main module through that pointer.
    static LOADER: wasm_split::LazyLoader<u32, u32> =
        wasm_split::lazy_loader!(extern "rich" fn factory(input: u32) -> u32);

    fn factory(pointer: u32) -> u32 {
        // The host allocates this vector in the main module. The lazy export
        // receives its linear-memory pointer and mutates those same bytes.
        // SAFETY: `call_cold` passes a live, writable 2048-u32 Vec allocation;
        // this function never retains the slice or changes its allocation.
        let values =
            unsafe { std::slice::from_raw_parts_mut(pointer as usize as *mut u32, WORKLOAD.len()) };
        let key = format!("cold:{}", values[0]).len() as u32;
        let mut checksum = 0u32;
        for (index, value) in values.iter_mut().enumerate() {
            *value = value.rotate_left(5).wrapping_mul(0x9e37_79b9) ^ WORKLOAD[index] ^ key;
            checksum = checksum.wrapping_add(*value);
        }
        checksum
    }

    #[unsafe(export_name = "main")]
    pub extern "C" fn bootstrap() -> u32 {
        42
    }

    #[wasm_bindgen]
    pub async fn activate_capabilities(required: u32) -> Result<(), JsValue> {
        if required != 1 {
            return Err(JsValue::from_str("unexpected proof mask"));
        }
        if !LOADER.load().await {
            return Err(JsValue::from_str("load failed"));
        }
        Ok(())
    }

    #[wasm_bindgen]
    pub fn format_probe(value: u32) -> String {
        format!("left:{value}:{}", "right")
    }

    #[wasm_bindgen]
    pub fn nested_format_probe(value: u32) -> String {
        format!("{} {}", "prefix", &format!("{}{}", "value", value))
    }

    #[wasm_bindgen]
    pub fn call_cold(input: u32) -> Result<Vec<u32>, JsValue> {
        let mut values: Vec<u32> =
            (0..WORKLOAD.len()).map(|index| input.wrapping_add(index as u32)).collect();
        // Passing a scalar pointer across the split boundary keeps the lazy
        // function ABI simple while proving both modules share linear memory.
        let _checksum = LOADER
            .call(values.as_mut_ptr() as usize as u32)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(values)
    }
}
