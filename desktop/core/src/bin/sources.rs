//! Publishing tool for the definitions in `sources/sources.json`.
//!
//!   akidev-sources keygen <key-file>                    make the signing key, print its public half
//!   akidev-sources sign <key-file> <json> <sig-file>    sign a document
//!   akidev-sources check <json> [sig-file]              validate it, and verify the signature if given

use akidev_core::sources;
use anyhow::{Context, bail};
use ed25519_dalek::{Signer, SigningKey};

fn load_key(path: &str) -> anyhow::Result<SigningKey> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let seed: [u8; 32] = sources::decode_hex(&text)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("the key is not 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Writes the private key where only its owner can read it.
fn write_private(path: &str, text: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Windows has no mode bits; the key lives in the user's own folder there.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("creating {path} (it must not exist already)"))?
        .write_all(text.as_bytes())?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", file] => {
            let mut seed = [0u8; 32];
            getrandom::fill(&mut seed)?;
            write_private(file, &sources::encode_hex(&seed))?;
            println!(
                "{}",
                sources::encode_hex(SigningKey::from_bytes(&seed).verifying_key().as_bytes())
            );
            eprintln!(
                "Private key written to {file}. Keep it secret and put the line above in PUBLIC_KEY in core/src/sources.rs."
            );
        }
        ["sign", key, document, signature] => {
            let bytes = std::fs::read(document).with_context(|| format!("reading {document}"))?;
            let sources = sources::parse(&bytes)?;
            std::fs::write(
                signature,
                sources::encode_hex(&load_key(key)?.sign(&bytes).to_bytes()),
            )?;
            println!("signed version {}", sources.version);
        }
        ["check", document] | ["check", document, _] => {
            let bytes = std::fs::read(document).with_context(|| format!("reading {document}"))?;
            let sources = sources::parse(&bytes)?;
            if let Some(signature) = args.get(2) {
                sources::verify(&bytes, &std::fs::read_to_string(signature)?)?;
            }
            println!(
                "version {}: {} indexers, {} providers{}",
                sources.version,
                sources.indexers.len(),
                sources.providers.len(),
                if args.len() > 2 {
                    ", signature valid"
                } else {
                    ""
                }
            );
        }
        _ => bail!(
            "usage: akidev-sources keygen <key-file> | sign <key-file> <json> <sig-file> | check <json> [sig-file]"
        ),
    }
    Ok(())
}
