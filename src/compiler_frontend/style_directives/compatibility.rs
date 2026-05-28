//! Template-head compatibility tags for style directive parsing.
//!
//! Parser state needs cheap set membership checks while still keeping compatibility policy on
//! directive specs. Named bit tags make combinations clearer than threading many boolean flags
//! through tokenizer and template parsing code.

use std::ops::{BitAnd, BitOr, BitOrAssign};

/// Template-head compatibility tags for directives and other meaningful head items.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TemplateHeadTag(u64);

impl TemplateHeadTag {
    pub const MEANINGFUL_ITEM: Self = Self(1 << 0);
    pub const SLOT_DIRECTIVE: Self = Self(1 << 1);
    pub const INSERT_DIRECTIVE: Self = Self(1 << 2);
    pub const COMMENT_DIRECTIVE: Self = Self(1 << 3);
    pub const FORMATTER_DIRECTIVE: Self = Self(1 << 4);
    pub const CHILDREN_DIRECTIVE: Self = Self(1 << 5);
    pub const FRESH_DIRECTIVE: Self = Self(1 << 6);
    pub const RAW_DIRECTIVE: Self = Self(1 << 7);
    pub const CODE_DIRECTIVE: Self = Self(1 << 8);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl BitOr for TemplateHeadTag {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TemplateHeadTag {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for TemplateHeadTag {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

/// Data-driven template-head compatibility rules attached to each directive spec.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TemplateHeadCompatibility {
    pub presence_tags: TemplateHeadTag,
    pub required_absent_tags: TemplateHeadTag,
    pub blocks_future_tags: TemplateHeadTag,
}

impl TemplateHeadCompatibility {
    pub fn fully_compatible_meaningful() -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            required_absent_tags: TemplateHeadTag::empty(),
            blocks_future_tags: TemplateHeadTag::empty(),
        }
    }

    pub fn exclusive_meaningful() -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
        }
    }

    pub fn blocks_same(tag: TemplateHeadTag) -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM | tag,
            required_absent_tags: TemplateHeadTag::empty(),
            blocks_future_tags: tag,
        }
    }
}
