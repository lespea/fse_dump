use fnv::FnvHashMap;

use std::sync::{Arc, RwLock};

// Generate the static lookups we need to translate numbers to strings
lazy_static! {
    // These are all of the flags that are defined
    // (from https://github.com/dlcowen/FSEventsParser/blob/master/FSEParser_V3.3.py)
    static ref FLAGS: Vec<(&'static str, u32)> = vec!(
        ("FolderEvent"         , 0x00000001),
        ("Mount"               , 0x00000002),
        ("Unmount"             , 0x00000004),
        ("EndOfTransaction"    , 0x00000020),
        ("LastHardLinkRemoved" , 0x00000800),
        ("HardLink"            , 0x00001000),
        ("SymbolicLink"        , 0x00004000),
        ("FileEvent"           , 0x00008000),
        ("PermissionChange"    , 0x00010000),
        ("ExtendedAttrModified", 0x00020000),
        ("ExtendedAttrRemoved" , 0x00040000),
        ("DocumentRevisioning" , 0x00100000),
        ("ItemCloned"          , 0x00400000),
        ("Created"             , 0x01000000),
        ("Removed"             , 0x02000000),
        ("InodeMetaMod"        , 0x04000000),
        ("Renamed"             , 0x08000000),
        ("Modified"            , 0x10000000),
        ("Exchange"            , 0x20000000),
        ("FinderInfoMod"       , 0x40000000),
        ("FolderCreated"       , 0x80000000),
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
    // Should be enough for every flag to be set (which shouldn't happen but just in case)
    let mut s = String::with_capacity(500);

    for (name, num) in FLAGS.iter() {
        if bits & *num == *num {
            if !s.is_empty() {
                s.push_str(", ")
            }
            s.push_str(*name)
        }
    }

    // Since these are long lived we might as well shrink this down to what's needed
    s.shrink_to_fit();
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
