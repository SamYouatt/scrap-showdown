//! Launch options: `?scene=fight&opp=2` style URL params on the web, or
//! `SCRAP_SCENE=fight SCRAP_OPP=2` style env vars on native.

pub fn param(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        for kv in search.trim_start_matches('?').split('&') {
            let mut it = kv.splitn(2, '=');
            if it.next() == Some(key) {
                return Some(it.next().unwrap_or("").to_string());
            }
        }
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var(format!("SCRAP_{}", key.to_uppercase())).ok()
    }
}
