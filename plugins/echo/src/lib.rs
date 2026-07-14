wit_bindgen::generate!({
    world: "plugin-world",
    path: "../../wit",
});

use exports::treetags::plugin::plugin::{Guest, Request, Tag};

struct EchoPlugin;

impl Guest for EchoPlugin {
    fn generate(req: Request, _source: Vec<u8>) -> Result<Vec<Tag>, String> {
        if let Some(cache_name) = req.cache_file {
            std::fs::write(&cache_name, "echo_cache_written\n")
                .map_err(|e| format!("cache write error: {e}"))?;
        }
        Ok(vec![Tag {
            name: "echo_tag".into(),
            line: 1,
            kind: "f".into(),
            end_line: None,
            extension_fields: vec![],
        }])
    }
}

export!(EchoPlugin);
