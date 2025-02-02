use super::*;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(fuzzing, derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum Parent {
    String(String),
    Id(Id),
}

#[rustfmt::skip]
#[allow(dead_code)]
pub mod item_flags {
    pub const ITEM_KEEP                 : u8 = 0b0000_0001;
    pub const ITEM_COUNTABLE            : u8 = 0b0000_0010;
    pub const ITEM_DELETED              : u8 = 0b0000_0100;
    pub const ITEM_MARKED               : u8 = 0b0000_1000;
    pub const ITEM_HAS_PARENT_SUB       : u8 = 0b0010_0000;
    pub const ITEM_HAS_RIGHT_ID         : u8 = 0b0100_0000;
    pub const ITEM_HAS_LEFT_ID          : u8 = 0b1000_0000;
    pub const ITEM_HAS_PARENT_INFO      : u8 = 0b1100_0000;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct ItemFlags(#[cfg_attr(test, proptest(value = 0))] u8);

impl From<u8> for ItemFlags {
    fn from(flags: u8) -> Self {
        Self(flags)
    }
}

impl ItemFlags {
    #[inline(always)]
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }

    #[inline(always)]
    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }

    #[inline(always)]
    pub fn check(&self, flag: u8) -> bool {
        self.0 & flag == flag
    }

    #[inline(always)]
    pub fn not(&self, flag: u8) -> bool {
        self.0 & flag == 0
    }

    #[inline(always)]
    pub fn keep(&self) -> bool {
        self.check(item_flags::ITEM_KEEP)
    }

    #[inline(always)]
    pub fn set_keep(&mut self) {
        self.set(item_flags::ITEM_KEEP);
    }

    #[inline(always)]
    pub fn clear_keep(&mut self) {
        self.clear(item_flags::ITEM_KEEP);
    }

    #[inline(always)]
    pub fn countable(&self) -> bool {
        self.check(item_flags::ITEM_COUNTABLE)
    }

    #[inline(always)]
    pub fn set_countable(&mut self) {
        self.set(item_flags::ITEM_COUNTABLE);
    }

    #[inline(always)]
    pub fn clear_countable(&mut self) {
        self.clear(item_flags::ITEM_COUNTABLE);
    }

    #[inline(always)]
    pub fn deleted(&self) -> bool {
        self.check(item_flags::ITEM_DELETED)
    }

    #[inline(always)]
    pub fn set_deleted(&mut self) {
        self.set(item_flags::ITEM_DELETED);
    }

    #[inline(always)]
    pub fn clear_deleted(&mut self) {
        self.clear(item_flags::ITEM_DELETED);
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Item {
    pub id: Id,
    pub left_id: Option<Id>,
    pub right_id: Option<Id>,
    pub parent: Option<Parent>,
    pub parent_sub: Option<String>,
    pub content: Content,
    pub flags: ItemFlags,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            id: Id::default(),
            left_id: None,
            right_id: None,
            parent: None,
            parent_sub: None,
            content: Content::String("".into()),
            flags: ItemFlags::from(item_flags::ITEM_COUNTABLE),
        }
    }
}

impl Item {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> u64 {
        self.content.clock_len()
    }

    pub fn deleted(&self) -> bool {
        self.flags.deleted()
    }

    pub fn delete(&mut self) {
        if self.deleted() {
            return;
        }

        // self.content.delete();

        self.flags.deleted();
    }
}

impl Item {
    pub(crate) fn read<R: CrdtReader>(
        decoder: &mut R,
        id: Id,
        info: u8,
        first_5_bit: u8,
    ) -> JwstCodecResult<Self> {
        let flags: ItemFlags = info.into();
        let has_left_id = flags.check(item_flags::ITEM_HAS_LEFT_ID);
        let has_right_id = flags.check(item_flags::ITEM_HAS_RIGHT_ID);
        let has_parent_sub = flags.check(item_flags::ITEM_HAS_PARENT_SUB);
        let has_not_parent_info = flags.not(item_flags::ITEM_HAS_PARENT_INFO);

        // NOTE: read order must keep the same as the order in yjs
        // TODO: this data structure design will break the cpu OOE, need to be optimized
        let mut item = Self {
            id,
            left_id: if has_left_id {
                Some(decoder.read_item_id()?)
            } else {
                None
            },
            right_id: if has_right_id {
                Some(decoder.read_item_id()?)
            } else {
                None
            },
            parent: {
                if has_not_parent_info {
                    let has_parent = decoder.read_var_u64()? == 1;
                    Some(if has_parent {
                        Parent::String(decoder.read_var_string()?)
                    } else {
                        Parent::Id(decoder.read_item_id()?)
                    })
                } else {
                    None
                }
            },
            parent_sub: if has_not_parent_info && has_parent_sub {
                Some(decoder.read_var_string()?)
            } else {
                None
            },
            content: {
                // tag must not GC or Skip, this must process in parse_struct
                debug_assert_ne!(first_5_bit, 0);
                debug_assert_ne!(first_5_bit, 10);
                Content::read(decoder, first_5_bit)?
            },
            flags: ItemFlags::from(0),
        };

        if item.content.countable() {
            item.flags.set_countable();
        }

        debug_assert!(item.is_valid());

        Ok(item)
    }

    fn get_info(&self) -> (u8, bool) {
        let mut info = self.content.get_info();
        if self.left_id.is_some() {
            info |= item_flags::ITEM_HAS_LEFT_ID;
        }
        if self.right_id.is_some() {
            info |= item_flags::ITEM_HAS_RIGHT_ID;
        }
        let has_not_parent_info = info & item_flags::ITEM_HAS_PARENT_INFO == 0;
        if has_not_parent_info && self.parent_sub.is_some() {
            info |= item_flags::ITEM_HAS_PARENT_SUB;
        }
        (info, has_not_parent_info)
    }

    pub(crate) fn is_valid(&self) -> bool {
        let has_id = self.left_id.is_some() || self.right_id.is_some();
        !has_id && self.parent.is_some()
            || has_id && self.parent.is_none() && self.parent_sub.is_none()
    }

    pub(crate) fn write<W: CrdtWriter>(&self, encoder: &mut W) -> JwstCodecResult {
        let (info, has_not_parent_info) = self.get_info();
        encoder.write_info(info)?;

        if let Some(left_id) = self.left_id {
            encoder.write_item_id(&left_id)?;
        }
        if let Some(right_id) = self.right_id {
            encoder.write_item_id(&right_id)?;
        }
        if has_not_parent_info {
            if let Some(parent) = &self.parent {
                match parent {
                    Parent::String(s) => {
                        encoder.write_var_u64(1)?;
                        encoder.write_var_string(s)?;
                    }
                    Parent::Id(id) => {
                        encoder.write_var_u64(0)?;
                        encoder.write_item_id(id)?;
                    }
                }
            } else {
                return Err(JwstCodecError::ParentNotFound);
            }
        }

        if let Some(parent_sub) = &self.parent_sub {
            if has_not_parent_info {
                encoder.write_var_string(parent_sub)?;
            }
        }

        self.content.write(encoder)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{collection::vec, prelude::*};

    fn item_round_trip(item: &mut Item) -> JwstCodecResult {
        if !item.is_valid() {
            return Ok(());
        }

        if item.content.countable() {
            item.flags.set_countable();
        }

        let mut encoder = RawEncoder::default();
        item.write(&mut encoder)?;

        let mut decoder = RawDecoder::new(encoder.into_inner());

        let info = decoder.read_info()?;
        let first_5_bit = info & 0b11111;
        let decoded_item = Item::read(&mut decoder, item.id.clone(), info, first_5_bit)?;

        assert_eq!(item, &decoded_item);

        Ok(())
    }

    proptest! {
        #[test]
        fn test_random_content(mut items in vec(any::<Item>(), 0..10)) {
            for item in &mut items {
                item_round_trip(item).unwrap();
            }
        }
    }
}
