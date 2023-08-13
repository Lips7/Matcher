use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};

use ahash::{AHashMap, AHashSet};
use bitflags::bitflags;
use hyperscan::{BlockDatabase, BlockScanner, Error, Flag, Pattern, Scan};
use nohash_hasher::{IntMap, IntSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tinyvec::{ArrayVec, TinyVec};
