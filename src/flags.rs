use fnv::FnvHashMap;

use std::sync::{Arc, RwLock};

const FLAG_SEP: &str = " | ";

// Generate the static lookups we need to translate numbers to strings
lazy_static! {
    // These are all of the flags that are defined
    // (from https://github.com/dlcowen/FSEventsParser/blob/master/FSEParser_V3.3.py)
    static ref FLAGS: Vec<(&'static str, u32)> = vec!(
        ("FolderEvent"         , 0x_0000_0001),
        ("Mount"               , 0x_0000_0002),
        ("Unmount"             , 0x_0000_0004),
        ("EndOfTransaction"    , 0x_0000_0020),
        ("LastHardLinkRemoved" , 0x_0000_0800),
        ("HardLink"            , 0x_0000_1000),
        ("SymbolicLink"        , 0x_0000_4000),
        ("FileEvent"           , 0x_0000_8000),
        ("PermissionChange"    , 0x_0001_0000),
        ("ExtendedAttrModified", 0x_0002_0000),
        ("ExtendedAttrRemoved" , 0x_0004_0000),
        ("DocumentRevisioning" , 0x_0010_0000),
        ("ItemCloned"          , 0x_0040_0000),
        ("Created"             , 0x_0100_0000),
        ("Removed"             , 0x_0200_0000),
        ("InodeMetaMod"        , 0x_0400_0000),
        ("Renamed"             , 0x_0800_0000),
        ("Modified"            , 0x_1000_0000),
        ("Exchange"            , 0x_2000_0000),
        ("FinderInfoMod"       , 0x_4000_0000),
        ("FolderCreated"       , 0x_8000_0000),
    );

    // Turn the flags into a lookup map since we'll cache all of the numbers we find while parsing
    // Because we can't guarantee that each entry will be around forever we need to wrap it in
    // an Arc.  The map itself is behind a rwLock so we can modify the entries when we find a flag
    // that hasn't been seen before
    static ref FLAG_MAP: RwLock<FnvHashMap<u32, Arc<String>>> = {
        let mut m = FnvHashMap::with_capacity_and_hasher(FLAGS.len() * 3, Default::default());

        for (name, num) in FLAGS.iter() {
            m.insert(*num, Arc::new((*name).to_owned()));
        }

        // We'll probably need this
        m.insert(0, Arc::new("".to_string()));

        RwLock::new(m)
    };
}

/// Turn the flag bits into a string. We simply enumerate the flags, see if it's set, and add the
/// str to the list of flags found so far (comma separated)
fn bits_to_str(bits: u32) -> String {
    debug!(target: "flags", "Figuring out the bits for {}", bits);
    // Should be enough for every flag to be set (which shouldn't happen but just in case)
    let mut s = String::with_capacity(500);

    for (name, num) in FLAGS.iter() {
        if bits & *num == *num {
            if !s.is_empty() {
                s.push_str(FLAG_SEP)
            }
            s.push_str(*name)
        }
    }

    // Since these are long lived we might as well shrink this down to what's needed
    s.shrink_to_fit();
    debug!(target:"flags", "Bits {} == {}", bits, s);
    s
}

/// Given the bits, return a string representing the flags that are set
pub fn parse_bits(bits: u32) -> Arc<String> {
    debug!(target:"flags", "Translating the bits {}", bits);
    let ans = {
        FLAG_MAP
            .read()
            .expect("Couldn't lock the lookup map?")
            .get(&bits)
            .cloned()
    };

    ans.unwrap_or_else(|| {
        debug!(target:"flags", "Trying lock");
        FLAG_MAP
            .write()
            .expect("Couldn't lock the lookup map?")
            .entry(bits)
            .or_insert_with(|| {
                debug!("Making new flag entry");
                Arc::new(bits_to_str(bits))
            })
            .clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_bits_to_strs() {
        let _ = env_logger::init();
        for (name, flag) in FLAGS.iter() {
            assert_eq!(bits_to_str(*flag), name.to_owned());
            assert_eq!(bits_to_str(*flag), (*name).to_owned());
        }
    }

    #[test]
    fn complex_bits_to_strs() {
        let _ = env_logger::init();
        let (combo_str, combo_num) = FLAGS.iter().take(3).fold(
            (String::with_capacity(500), 0u32),
            |(mut string, flag), (new_str, new_flag)| {
                if !string.is_empty() {
                    string.push_str(FLAG_SEP);
                }
                string.push_str(*new_str);
                (string, flag | new_flag)
            },
        );
        assert_eq!(bits_to_str(combo_num), combo_str);
    }

    #[test]
    fn simple_parse_bits() {
        let _ = env_logger::init();
        for (name, flag) in FLAGS.iter() {
            assert_eq!(*parse_bits(*flag), (*name).to_owned());
            assert_eq!(*parse_bits(*flag), (*name).to_owned());

            assert_eq!(*parse_bits(*flag), name.to_owned());
            assert_eq!(*parse_bits(*flag), name.to_owned());
        }
    }

    #[test]
    fn complex_parse_bits() {
        let _ = env_logger::init();
        let (combo_str, combo_num) = FLAGS.iter().take(3).fold(
            (String::with_capacity(500), 0u32),
            |(mut string, flag), (new_str, new_flag)| {
                if !string.is_empty() {
                    string.push_str(FLAG_SEP);
                }
                string.push_str(*new_str);
                (string, flag | new_flag)
            },
        );

        assert_eq!(*parse_bits(combo_num), combo_str);
        assert_eq!(*parse_bits(combo_num), combo_str);
    }
}
