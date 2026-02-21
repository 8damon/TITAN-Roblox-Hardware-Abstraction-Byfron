use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use rand::distributions::Alphanumeric;
use rand::{Rng, thread_rng};
use tracing::{info, trace, warn};

fn gen_rand() -> String {
    let mut rng = thread_rng();

    (0..32)
        .map(|_| rng.sample(Alphanumeric) as char)
        .collect::<String>()
        .to_uppercase()
}

pub fn mutate_referents(file_path: &Path, item_class: &str) -> io::Result<()> {
    if !file_path.exists() {
        trace!(?file_path, "Referent file not found");
        return Ok(());
    }

    info!(?file_path, class = %item_class, "Mutating referents");

    let mut file = File::open(file_path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let new_id = gen_rand();
    let (new_content, replacements) = replace_referent_markers(&content, item_class, &new_id);

    if replacements == 0 {
        warn!(?file_path, "No referent entries matched");
    } else {
        info!(?file_path, new_id = %new_id, replacements, "Referent mutated");
    }

    let mut out = File::create(file_path)?;
    out.write_all(new_content.as_bytes())?;

    Ok(())
}

fn replace_referent_markers(content: &str, item_class: &str, new_id: &str) -> (String, usize) {
    let prefix = format!(r#"<Item class="{}" referent=""#, item_class);
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0usize;
    let mut replaced = 0usize;

    while let Some(found_rel) = content[cursor..].find(&prefix) {
        let found = cursor + found_rel;
        let referent_start = found + prefix.len();
        let after_referent = &content[referent_start..];
        let Some(end_quote_rel) = after_referent.find('"') else {
            break;
        };
        let end_quote = referent_start + end_quote_rel;
        let post_quote = content.as_bytes().get(end_quote + 1).copied();
        if post_quote != Some(b'>') {
            cursor = end_quote + 1;
            continue;
        }

        out.push_str(&content[cursor..referent_start]);
        out.push_str(new_id);
        cursor = end_quote;
        replaced = replaced.saturating_add(1);
    }

    out.push_str(&content[cursor..]);
    (out, replaced)
}
