//! Binary-format embedded signature section extraction.
//!
//! This microcrate isolates PE/ELF/Mach-O section scanning and returns raw JSON
//! payload bytes so higher-level crates can deserialize into their own metadata types.

#![deny(static_mut_refs)]
#![deny(unsafe_op_in_unsafe_fn, clippy::unwrap_used)]

use anyhow::Context;
use std::path::Path;

/// Section name for embedded signatures in PE binaries (Windows)
pub const PE_SIGNATURE_SECTION: &str = ".orsig";

/// Section name for embedded signatures in ELF binaries (Linux)
pub const ELF_SIGNATURE_SECTION: &str = ".note.openracing.sig";

/// Section name for embedded signatures in Mach-O binaries (macOS)
pub const MACHO_SIGNATURE_SECTION: &str = "__orsig";

/// Mach-O segment containing the signature section
pub const MACHO_SIGNATURE_SEGMENT: &str = "__DATA";

/// Extract raw embedded signature payload bytes from a binary.
///
/// Returns `Ok(Some(bytes))` when an embedded signature section exists and is non-empty,
/// `Ok(None)` when no compatible section is found, and `Err` on I/O errors.
pub fn extract_embedded_signature_payload(file_path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    use goblin::Object;
    use tracing::debug;

    let file_data = std::fs::read(file_path)
        .with_context(|| format!("Failed to read file for signature extraction: {file_path:?}"))?;

    let object = match Object::parse(&file_data) {
        Ok(obj) => obj,
        Err(_) => return Ok(None),
    };

    let section_data = match object {
        Object::PE(pe) => {
            debug!("Checking PE binary for embedded signature");
            extract_pe_signature_section(&pe, &file_data)
        }
        Object::Elf(elf) => {
            debug!("Checking ELF binary for embedded signature");
            extract_elf_signature_section(&elf, &file_data)
        }
        Object::Mach(mach) => {
            debug!("Checking Mach-O binary for embedded signature");
            extract_macho_signature_section(&mach, &file_data)
        }
        _ => None,
    };

    Ok(section_data.map(ToOwned::to_owned))
}

fn extract_pe_signature_section<'a>(
    pe: &goblin::pe::PE<'_>,
    file_data: &'a [u8],
) -> Option<&'a [u8]> {
    for section in &pe.sections {
        let name = String::from_utf8_lossy(&section.name);
        let name = name.trim_end_matches('\0');
        if name == PE_SIGNATURE_SECTION {
            let start = section.pointer_to_raw_data as usize;
            let size = section.size_of_raw_data as usize;
            if start + size <= file_data.len() {
                let data = &file_data[start..start + size];
                let trimmed = trim_null_bytes(data);
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn extract_elf_signature_section<'a>(
    elf: &goblin::elf::Elf<'_>,
    file_data: &'a [u8],
) -> Option<&'a [u8]> {
    for section in &elf.section_headers {
        if let Some(name) = elf.shdr_strtab.get_at(section.sh_name)
            && name == ELF_SIGNATURE_SECTION
        {
            let start = section.sh_offset as usize;
            let size = section.sh_size as usize;
            if start + size <= file_data.len() {
                let data = &file_data[start..start + size];
                let trimmed = trim_null_bytes(data);
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

fn extract_macho_signature_section<'a>(
    mach: &goblin::mach::Mach<'_>,
    file_data: &'a [u8],
) -> Option<&'a [u8]> {
    match mach {
        goblin::mach::Mach::Binary(macho) => extract_macho_binary_signature(macho, file_data),
        goblin::mach::Mach::Fat(fat) => {
            for arch in fat.iter_arches().flatten() {
                if let Ok(macho) = goblin::mach::MachO::parse(file_data, arch.offset as usize)
                    && let Some(data) = extract_macho_binary_signature(&macho, file_data)
                {
                    return Some(data);
                }
            }
            None
        }
    }
}

fn extract_macho_binary_signature<'a>(
    macho: &goblin::mach::MachO<'_>,
    file_data: &'a [u8],
) -> Option<&'a [u8]> {
    for segment in &macho.segments {
        if let Ok(name) = segment.name()
            && name == MACHO_SIGNATURE_SEGMENT
            && let Ok(sections) = segment.sections()
        {
            for (section, _data) in sections {
                if let Ok(sect_name) = section.name()
                    && sect_name == MACHO_SIGNATURE_SECTION
                {
                    let start = section.offset as usize;
                    let size = section.size as usize;
                    if start + size <= file_data.len() {
                        let data = &file_data[start..start + size];
                        let trimmed = trim_null_bytes(data);
                        if !trimmed.is_empty() {
                            return Some(trimmed);
                        }
                    }
                }
            }
        }
    }
    None
}

fn trim_null_bytes(data: &[u8]) -> &[u8] {
    let end = data.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &data[..end]
}

#[cfg(test)]
mod tests {
    use super::trim_null_bytes;

    #[test]
    fn trim_null_bytes_removes_trailing_padding() {
        let source = b"{\"k\":1}\0\0\0";
        assert_eq!(trim_null_bytes(source), b"{\"k\":1}");
    }
}
