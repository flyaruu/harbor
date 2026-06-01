#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(not(target_arch = "wasm32"))]
fn native_cli_arg_value(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
            return (!value.is_empty()).then(|| value.to_owned());
        }

        if arg == flag {
            return args.next().filter(|value| !value.is_empty());
        }
    }

    None
}

#[cfg(target_arch = "wasm32")]
pub fn browser_runtime_config_value(browser_key: &str) -> Option<String> {
    query_param_value(browser_key).or_else(|| injected_config_value(browser_key))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_cli_spacetimedb_uri() -> Option<String> {
    static SPACETIMEDB_URI: OnceLock<Option<String>> = OnceLock::new();

    SPACETIMEDB_URI
        .get_or_init(|| native_cli_arg_value("--url"))
        .clone()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_cli_tile_server_uri() -> Option<String> {
    static TILE_SERVER_URI: OnceLock<Option<String>> = OnceLock::new();

    TILE_SERVER_URI
        .get_or_init(|| native_cli_arg_value("--tile-url"))
        .clone()
}

#[cfg(target_arch = "wasm32")]
fn query_param_value(browser_key: &str) -> Option<String> {
    use web_sys::{UrlSearchParams, window};

    let window = window()?;
    let search = window.location().search().ok()?;
    let params = UrlSearchParams::new_with_str(&search).ok()?;
    params.get(browser_key)
}

#[cfg(target_arch = "wasm32")]
fn injected_config_value(browser_key: &str) -> Option<String> {
    use js_sys::{Reflect, global};
    use web_sys::wasm_bindgen::JsValue;

    let config = Reflect::get(&global(), &JsValue::from_str("__HARBOR_RUNTIME_CONFIG__")).ok()?;
    let value = Reflect::get(&config, &JsValue::from_str(browser_key)).ok()?;
    value.as_string().filter(|value| !value.is_empty())
}
