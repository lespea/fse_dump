extern crate serde;
extern crate serde_json;

use std::borrow::Cow;

option_set! {
    pub struct RecordFlags: UpperCamel + u32 {
        const FOLDER_EVENT           = 0x00000001;
        const MOUNT                  = 0x00000002;
        const UNMOUNT                = 0x00000004;
        const END_OF_TRANSACTION     = 0x00000020;
        const LAST_HARD_LINK_REMOVED = 0x00000800;
        const HARD_LINK              = 0x00001000;
        const SYMBOLIC_LINK          = 0x00004000;
        const FILE_EVENT             = 0x00008000;
        const PERMISSION_CHANGE      = 0x00010000;
        const EXTENDED_ATTR_MODIFIED = 0x00020000;
        const EXTENDED_ATTR_REMOVED  = 0x00040000;
        const DOCUMENT_REVISIONING   = 0x00100000;
        const ITEM_CLONED            = 0x00400000;
        const CREATED                = 0x01000000;
        const REMOVED                = 0x02000000;
        const INODE_META_MOD         = 0x04000000;
        const RENAMED                = 0x08000000;
        const MODIFIED               = 0x10000000;
        const EXCHANGE               = 0x20000000;
        const FINDER_INFO_MOD        = 0x40000000;
        const FOLDER_CREATED         = 0x80000000;
    }
}

#[derive(Debug, Serialize)]
pub enum Flags {
    RF(RecordFlags),
    Str(String),
}

#[derive(Debug, Serialize)]
pub struct Record<'a> {
    pub path: Cow<'a, str>,
    pub event_id: u64,
    #[serde(flatten)]
    pub flags: Flags,
    pub node_id: Option<u64>,
}

impl<'a> Record<'a> {
    pub fn set_flags(&mut self, bits: u32) {
        match self.flags {
            Flags::RF(mut rf) => rf.bits = bits,
            _ => {
                let mut f = RecordFlags::default();
                f.bits = bits;
                self.flags = Flags::RF(f);
            },
        }
    }

    pub fn flatten_flags(&mut self) {
        match self.flags {
            Flags::RF(rf) => {
                rf.
            },

            _ => (),
        }
    }
}
