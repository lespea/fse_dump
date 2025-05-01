use hashbrown::HashMap;
use std::sync::{OnceLock, RwLock};

const FLAG_SEP: &str = " | ";

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
    // Should be enough for every flag to be set (which shouldn't happen but just in case)
    let mut norm = String::with_capacity(500);

    for (name, num) in FLAGS.iter() {
        if bits & *num == *num {
            if !norm.is_empty() {
                norm.push_str(FLAG_SEP)
            }
            norm.push_str(name)
        }
    }

    #[cfg(feature = "alt_flags")]
    let mut alt = String::with_capacity(500);

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
            (String::with_capacity(500), 0u32),
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
            (String::with_capacity(500), 0u32),
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
}
