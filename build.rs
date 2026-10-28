#[cfg(not(any(feature = "smartcn", feature = "nori", feature = "kuromoji")))]
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(any(feature = "smartcn", feature = "nori", feature = "kuromoji"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    dictionaries::build()
}

#[cfg(any(feature = "smartcn", feature = "nori", feature = "kuromoji"))]
mod dictionaries {
    use std::{env, error::Error, fs, io::Cursor, path::PathBuf};

    use flate2::read::GzDecoder;
    use sha2::{Digest, Sha256};

    const RELEASE: &str = "https://github.com/turbopuffer/alyze/releases/download/dictionaries-v1";
    const DICTIONARIES: &[(&str, &str, &[&str])] = &[
        (
            "smartcn",
            "21d22bf36ff94e450d98f7c8d48bfdc336ef717fed26b5a6a741904968f98367",
            &["coredict.bin", "bigramdict.bin"],
        ),
        (
            "nori",
            "9fa5de2a769010e57462a036327012dbbaca53d4e3a8c70ad15b68c4e0f022e2",
            &[
                "nori/chardef.bin",
                "nori/costs.bin",
                "nori/terms.fst",
                "nori/unk.bin",
                "nori/words.bin",
            ],
        ),
        (
            "kuromoji",
            "ec9ae868e7c7ebc20109ed0c318515a8df234fdaab1a485bf72734f04fd8ac3b",
            &[
                "kuromoji/chardef.bin",
                "kuromoji/costs.bin",
                "kuromoji/ids.bin",
                "kuromoji/romaji_map.txt",
                "kuromoji/terms.fst",
                "kuromoji/unk.bin",
                "kuromoji/words.bin",
            ],
        ),
    ];

    pub fn build() -> Result<(), Box<dyn Error>> {
        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rerun-if-env-changed=ALYZE_CJK_DICTIONARIES");
        println!("cargo:rerun-if-env-changed=DOCS_RS");
        let local = env::var_os("ALYZE_CJK_DICTIONARIES").map(PathBuf::from);
        let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("data");
        let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        let docs = env::var_os("DOCS_RS").is_some();
        let use_source = !docs && local.is_none() && source.is_dir();
        let data = if use_source {
            source
        } else {
            output.join("data")
        };
        println!("cargo:rustc-env=ALYZE_CJK_DATA={}", data.display());

        for &(language, checksum, files) in DICTIONARIES {
            if env::var_os(format!("CARGO_FEATURE_{}", language.to_uppercase())).is_none() {
                continue;
            }
            if docs {
                for file in files {
                    let path = data.join(file);
                    fs::create_dir_all(path.parent().unwrap())?;
                    fs::write(path, [])?;
                }
            } else if use_source {
                for file in files {
                    let path = data.join(file);
                    if !path.is_file() {
                        return Err(format!("missing dictionary file: {}", path.display()).into());
                    }
                    println!("cargo:rerun-if-changed={}", path.display());
                }
            } else {
                let name = format!("{language}.tar.gz");
                let archive = match &local {
                    Some(directory) => directory.join(&name),
                    None => output.join(&name),
                };
                if local.is_some() {
                    println!("cargo:rerun-if-changed={}", archive.display());
                }
                let bytes = if local.is_some() || archive.is_file() {
                    fs::read(&archive)?
                } else {
                    println!("cargo:warning=Downloading {name}");
                    ureq::get(&format!("{RELEASE}/{name}"))
                    .config()
                    .timeout_global(Some(std::time::Duration::from_secs(120)))
                    .build()
                    .call()
                    .map_err(|error| format!("cannot download {name}: {error}; set ALYZE_CJK_DICTIONARIES to a directory containing the release archives for offline builds"))?
                    .body_mut()
                    .with_config()
                    .limit(64 * 1024 * 1024)
                    .read_to_vec()?
                };
                let actual = format!("{:x}", Sha256::digest(&bytes));
                if actual != checksum {
                    return Err(format!(
                        "SHA-256 mismatch for {name}: expected {checksum}, got {actual}"
                    )
                    .into());
                }
                if local.is_none() {
                    fs::write(&archive, &bytes)?;
                }
                tar::Archive::new(GzDecoder::new(Cursor::new(bytes))).unpack(&data)?;
                for file in files {
                    if !data.join(file).is_file() {
                        return Err(format!("{name} is missing {file}").into());
                    }
                }
            }
        }
        Ok(())
    }
}
