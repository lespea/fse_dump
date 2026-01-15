//! FSEvents flag definitions and parsing utilities
//!
//! This module provides flag bit definitions for macOS FSEvents and utilities
//! to convert between bit representations and human-readable strings.
//! Supports both standard and alternative flag interpretations.

use hashbrown::HashMap;
use std::sync::{OnceLock, RwLock};

const FLAG_SEP: &str = " | ";

/// Initial string capacity for flag strings
/// Calculated as: max_flag_name_length (23 for "ExtendedAttrModified") * max_flags (21)
/// + separator_length (3) * (max_flags - 1) = ~540 bytes, rounded to 512 for alignment
const FLAG_STRING_CAPACITY: usize = 512;

// These are all of the flags that are defined
// (from https://github.com/dlcowen/FSEventsParser/blob/master/FSEParser_V3.3.py)
static FLAGS: [(&str, u32); 21] = [
    ("FolderEvent", 0x_0000_0001),
    ("Mount", 0x_0000_0002),
    ("Unmount", 0x_0000_0004),
    ("EndOfTransaction", 0x_0000_0020),
    ("LastHardLinkRemoved", 0x_0000_0800),
    ("HardLink", 0x_0000_1000),
    ("SymbolicLink", 0x_0000_4000),
    ("FileEvent", 0x_0000_8000),
    ("PermissionChange", 0x_0001_0000),
    ("ExtendedAttrModified", 0x_0002_0000),
    ("ExtendedAttrRemoved", 0x_0004_0000),
    ("DocumentRevisioning", 0x_0010_0000),
    ("ItemCloned", 0x_0040_0000),
    ("Created", 0x_0100_0000),
    ("Removed", 0x_0200_0000),
    ("InodeMetaMod", 0x_0400_0000),
    ("Renamed", 0x_0800_0000),
    ("Modified", 0x_1000_0000),
    ("Exchange", 0x_2000_0000),
    ("FinderInfoMod", 0x_4000_0000),
    ("FolderCreated", 0x_8000_0000),
];

// Alt flags from https://github.com/ydkhatri/mac_apt/blob/master/plugins/fsevents.py
#[cfg(feature = "alt_flags")]
static ALT_FLAGS: [(&str, u32); 22] = [
    // ("None", 0x_0000_0000),
    ("Created", 0x_0000_0001),
    ("Removed", 0x_0000_0002),
    ("InodeMetaMod", 0x_0000_0004),
    ("RenamedOrMoved", 0x_0000_0008),
    ("Modified", 0x_0000_0010),
    ("Exchange", 0x_0000_0020),
    ("FinderInfoMod", 0x_0000_0040),
    ("FolderCreated", 0x_0000_0080),
    ("PermissionChange", 0x_0000_0100),
    ("XAttrModified", 0x_0000_0200),
    ("XAttrRemoved", 0x_0000_0400),
    ("0x00000800", 0x_0000_0800),
    ("DocumentRevision", 0x_0000_1000),
    // ("0x00002000", 0x_0000_2000),
    ("ItemCloned", 0x_0000_4000),
    // ("0x00008000", 0x_0000_8000),
    // ("0x00010000", 0x_0001_0000),
    // ("0x00020000", 0x_0002_0000),
    // ("0x00040000", 0x_0004_0000),
    ("LastHardLinkRemoved", 0x_0008_0000),
    ("HardLink", 0x_0010_0000),
    // ("0x00200000", 0x_0020_0000),
    ("SymbolicLink", 0x_0040_0000),
    ("FileEvent", 0x_0080_0000),
    ("FolderEvent", 0x_0100_0000),
    ("Mount", 0x_0200_0000),
    ("Unmount", 0x_0400_0000),
    // ("0x08000000", 0x_0800_0000),
    // ("0x10000000", 0x_1000_0000),
    ("EndOfTransaction", 0x_2000_0000),
    // ("0x40000000", 0x_4000_0000),
    // ("0x80000000", 0x_8000_0000),
];

/// Looks up a flag ID by its name (case-insensitive)
///
/// # Arguments
/// * `want` - The flag name to search for
///
/// # Returns
/// Some(u32) with the flag's bit value, or None if not found
pub fn flag_id(want: &str) -> Option<u32> {
    FLAGS.iter().find_map(|(name, id)| {
        if name.eq_ignore_ascii_case(want) {
            Some(*id)
        } else {
            None
        }
    })
}

/// Container for both normal and alternative flag string representations
#[derive(Clone, Copy, Debug, Default)]
pub struct FlagStrs {
    pub norm: &'static str,
    #[cfg(feature = "alt_flags")]
    pub alt: &'static str,
}

// Turn the flags into a lookup map since we'll cache all of the numbers we find while parsing
// Because we can't guarantee that each entry will be around forever we need to wrap it in
// an Arc.  The map itself is behind a rwLock so we can modify the entries when we find a flag
// that hasn't been seen before
static FLAG_MAP: OnceLock<RwLock<HashMap<u32, FlagStrs>>> = OnceLock::new();

/// Returns the global flag mapping cache
///
/// Initializes the cache on first access with all known flag combinations
pub fn flag_map() -> &'static RwLock<HashMap<u32, FlagStrs>> {
    FLAG_MAP.get_or_init(|| {
        let mut base: HashMap<u32, &'static str> = HashMap::with_capacity(FLAGS.len());

        for (name, num) in FLAGS.iter() {
            base.insert(*num, *name);
        }

        let mut combo = HashMap::with_capacity(128);

        // We'll probably need this
        combo.insert(0, FlagStrs::default());

        #[cfg(feature = "alt_flags")]
        for (alt, num) in ALT_FLAGS.iter() {
            if let Some(old) = combo.insert(
                *num,
                FlagStrs {
                    norm: base.remove(num).unwrap_or_default(),
                    alt,
                },
            ) {
                panic!("Dupe key? {num}/{old:?}")
            }
        }

        for (num, b) in base.into_iter() {
            if let Some(old) = combo.insert(
                num,
                FlagStrs {
                    norm: b,
                    #[cfg(feature = "alt_flags")]
                    alt: "",
                },
            ) {
                panic!("Dupe key? {num}/{old:?}")
            }
        }

        RwLock::new(combo)
    })
}

/// Turn the flag bits into a string. We simply enumerate the flags, see if it's set, and add the
/// str to the list of flags found so far (comma separated)
fn bits_to_str(bits: u32) -> FlagStrs {
    debug!(target: "flags", "Figuring out the bits for {bits}" );
    let mut norm = String::with_capacity(FLAG_STRING_CAPACITY);

    for (name, num) in FLAGS.iter() {
        if bits & *num == *num {
            if !norm.is_empty() {
                norm.push_str(FLAG_SEP)
            }
            norm.push_str(name)
        }
    }

    #[cfg(feature = "alt_flags")]
    let mut alt = String::with_capacity(FLAG_STRING_CAPACITY);

    #[cfg(feature = "alt_flags")]
    for (name, num) in ALT_FLAGS.iter() {
        if bits & *num == *num {
            if !alt.is_empty() {
                alt.push_str(FLAG_SEP)
            }
            alt.push_str(name)
        }
    }

    // Since these are long lived we might as well shrink this down to what's needed
    norm.shrink_to_fit();
    #[cfg(feature = "alt_flags")]
    {
        alt.shrink_to_fit();
        debug!(target: "flags", "Bits {} == {}/{}", bits, norm, alt);
    }

    FlagStrs {
        norm: Box::leak(norm.into_boxed_str()),
        #[cfg(feature = "alt_flags")]
        alt: Box::leak(alt.into_boxed_str()),
    }
}

/// Given the bits, return a string representing the flags that are set
pub fn parse_bits(bits: u32) -> FlagStrs {
    debug!(target: "flags", "Translating the bits {bits}" );
    let ans = {
        flag_map()
            .read()
            .expect("Couldn't lock the lookup map?")
            .get(&bits)
            .copied()
    };

    ans.unwrap_or_else(|| {
        debug!(target: "flags", "Trying lock");
        *flag_map()
            .write()
            .expect("Couldn't lock the lookup map?")
            .entry(bits)
            .or_insert_with(|| {
                debug!("Making new flag entry");
                bits_to_str(bits)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flag_id_case_insensitive() {
        assert_eq!(flag_id("Modified"), Some(0x1000_0000));
        assert_eq!(flag_id("modified"), Some(0x1000_0000));
        assert_eq!(flag_id("MODIFIED"), Some(0x1000_0000));
        assert_eq!(flag_id("MoDiFiEd"), Some(0x1000_0000));
    }

    #[test]
    fn test_flag_id_all_flags() {
        for (name, expected_id) in FLAGS.iter() {
            let result = flag_id(name);
            assert_eq!(
                result,
                Some(*expected_id),
                "Flag '{}' should return correct ID",
                name
            );
        }
    }

    #[test]
    fn test_flag_id_unknown() {
        assert_eq!(flag_id("UnknownFlag"), None);
        assert_eq!(flag_id("DoesNotExist"), None);
        assert_eq!(flag_id(""), None);
    }

    #[test]
    fn test_flag_id_specific_values() {
        assert_eq!(flag_id("Created"), Some(0x0100_0000));
        assert_eq!(flag_id("Removed"), Some(0x0200_0000));
        assert_eq!(flag_id("Renamed"), Some(0x0800_0000));
        assert_eq!(flag_id("FileEvent"), Some(0x0000_8000));
        assert_eq!(flag_id("FolderEvent"), Some(0x0000_0001));
    }

    #[test]
    fn simple_bits_to_strs() {
        let _ = env_logger::try_init();
        for (name, flag) in FLAGS.iter() {
            assert_eq!(bits_to_str(*flag).norm, *name);
            assert_eq!(bits_to_str(*flag).norm, *name);
        }
        #[cfg(feature = "alt_flags")]
        for (name, flag) in ALT_FLAGS.iter() {
            assert_eq!(bits_to_str(*flag).alt, *name);
            assert_eq!(bits_to_str(*flag).alt, *name);
        }
    }

    #[test]
    fn complex_bits_to_strs() {
        let _ = env_logger::try_init();
        let (combo_str, combo_num) = FLAGS.iter().take(3).fold(
            (String::with_capacity(FLAG_STRING_CAPACITY), 0u32),
            |(mut string, flag), (new_str, new_flag)| {
                if !string.is_empty() {
                    string.push_str(FLAG_SEP);
                }
                string.push_str(new_str);
                (string, flag | new_flag)
            },
        );
        assert_eq!(bits_to_str(combo_num).norm, combo_str);
    }

    #[test]
    fn simple_parse_bits() {
        let _ = env_logger::try_init();
        for (name, flag) in FLAGS.iter() {
            assert_eq!(*parse_bits(*flag).norm, **name);
            assert_eq!(*parse_bits(*flag).norm, **name);

            assert_eq!(parse_bits(*flag).norm, *name);
            assert_eq!(parse_bits(*flag).norm, *name);
        }
    }

    #[test]
    fn complex_parse_bits() {
        let _ = env_logger::try_init();
        let (combo_str, combo_num) = FLAGS.iter().take(3).fold(
            (String::with_capacity(FLAG_STRING_CAPACITY), 0u32),
            |(mut string, flag), (new_str, new_flag)| {
                if !string.is_empty() {
                    string.push_str(FLAG_SEP);
                }
                string.push_str(new_str);
                (string, flag | new_flag)
            },
        );

        assert_eq!(*parse_bits(combo_num).norm, combo_str);
        assert_eq!(*parse_bits(combo_num).norm, combo_str);
    }

    #[test]
    fn test_parse_bits_zero() {
        let result = parse_bits(0);
        assert_eq!(result.norm, "", "Zero bits should return empty string");
    }

    #[test]
    fn test_parse_bits_single_flags() {
        assert_eq!(parse_bits(0x0000_0001).norm, "FolderEvent");
        assert_eq!(parse_bits(0x0000_0002).norm, "Mount");
        assert_eq!(parse_bits(0x0000_0004).norm, "Unmount");
        assert_eq!(parse_bits(0x0000_8000).norm, "FileEvent");
        assert_eq!(parse_bits(0x0100_0000).norm, "Created");
        assert_eq!(parse_bits(0x0200_0000).norm, "Removed");
        assert_eq!(parse_bits(0x1000_0000).norm, "Modified");
        assert_eq!(parse_bits(0x8000_0000).norm, "FolderCreated");
    }

    #[test]
    fn test_parse_bits_multiple_flags() {
        // Created | Modified
        let result = parse_bits(0x0100_0000 | 0x1000_0000);
        assert!(result.norm.contains("Created"));
        assert!(result.norm.contains("Modified"));
        assert!(result.norm.contains(" | "));

        // FileEvent | Renamed | Removed
        let result = parse_bits(0x0000_8000 | 0x0800_0000 | 0x0200_0000);
        assert!(result.norm.contains("FileEvent"));
        assert!(result.norm.contains("Renamed"));
        assert!(result.norm.contains("Removed"));
    }

    #[test]
    fn test_parse_bits_all_flags() {
        let mut all_flags = 0u32;
        for (_, flag) in FLAGS.iter() {
            all_flags |= flag;
        }

        let result = parse_bits(all_flags);
        for (name, _) in FLAGS.iter() {
            assert!(result.norm.contains(name), "Should contain flag: {}", name);
        }
    }

    #[test]
    fn test_parse_bits_caching() {
        // First call should cache the result
        let bits = 0x1000_8000; // Modified | FileEvent
        let result1 = parse_bits(bits);

        // Second call should return cached result (same pointer)
        let result2 = parse_bits(bits);

        assert_eq!(result1.norm, result2.norm);
        assert!(
            std::ptr::eq(result1.norm, result2.norm),
            "Should return same cached string"
        );
    }

    #[test]
    fn test_parse_bits_unknown_bits() {
        // Use bits that aren't in the FLAGS array
        let unknown_bits = 0x0000_0100 | 0x0000_0200; // Not defined flags
        let result = parse_bits(unknown_bits);

        // Should still work, just won't match any known flags
        // Result should be empty or contain only the separator if no flags match
        assert!(!result.norm.contains("Unknown"));
    }

    #[test]
    fn test_flag_map_initialization() {
        let map = flag_map();
        let guard = map.read().expect("Should be able to read flag map");

        // Should have at least the zero entry and all FLAGS entries
        assert!(guard.len() >= FLAGS.len());
        assert!(guard.contains_key(&0), "Should contain zero key");
    }

    #[test]
    fn test_flag_map_contains_all_flags() {
        // Trigger initialization by parsing some flags
        for (_, flag_bits) in FLAGS.iter() {
            parse_bits(*flag_bits);
        }

        let map = flag_map();
        let guard = map.read().expect("Should be able to read flag map");

        for (name, flag_bits) in FLAGS.iter() {
            assert!(
                guard.contains_key(flag_bits),
                "Map should contain key for flag: {}",
                name
            );
        }
    }

    #[test]
    fn test_flag_strs_default() {
        let strs = FlagStrs::default();
        assert_eq!(strs.norm, "");
        #[cfg(feature = "alt_flags")]
        assert_eq!(strs.alt, "");
    }

    #[test]
    fn test_flag_separator() {
        let bits = 0x0000_0001 | 0x0000_0002; // FolderEvent | Mount
        let result = parse_bits(bits);
        assert!(
            result.norm.contains(FLAG_SEP),
            "Should contain separator between flags"
        );
    }

    #[test]
    fn test_bits_to_str_consistency() {
        // Test that bits_to_str produces consistent results
        let test_bits = 0x1000_8000; // Modified | FileEvent

        let result1 = bits_to_str(test_bits);
        let result2 = bits_to_str(test_bits);

        assert_eq!(
            result1.norm, result2.norm,
            "bits_to_str should be deterministic"
        );
    }

    #[test]
    fn test_flag_order_in_string() {
        // Flags should appear in the order they're defined in FLAGS array
        let bits = 0x0000_0001 | 0x0000_0002 | 0x0000_0004; // First 3 flags
        let result = parse_bits(bits);

        let first_flag_pos = result.norm.find(FLAGS[0].0);
        let second_flag_pos = result.norm.find(FLAGS[1].0);
        let third_flag_pos = result.norm.find(FLAGS[2].0);

        assert!(first_flag_pos.is_some());
        assert!(second_flag_pos.is_some());
        assert!(third_flag_pos.is_some());
        assert!(first_flag_pos < second_flag_pos);
        assert!(second_flag_pos < third_flag_pos);
    }

    #[cfg(feature = "alt_flags")]
    #[test]
    fn test_alt_flags_parsing() {
        for (name, flag) in ALT_FLAGS.iter() {
            let result = parse_bits(*flag);
            assert_eq!(
                result.alt, *name,
                "Alt flag '{}' should parse correctly",
                name
            );
        }
    }

    #[cfg(feature = "alt_flags")]
    #[test]
    fn test_alt_flags_multiple() {
        let bits = ALT_FLAGS[0].1 | ALT_FLAGS[1].1;
        let result = parse_bits(bits);

        assert!(result.alt.contains(ALT_FLAGS[0].0));
        assert!(result.alt.contains(ALT_FLAGS[1].0));
    }
}
