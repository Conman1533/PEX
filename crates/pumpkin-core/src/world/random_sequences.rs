use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};

use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::random::RandomImpl;
use pumpkin_util::random::xoroshiro128::Xoroshiro;

/// A single random sequence wrapper.
pub struct RandomSequence {
    rng: Xoroshiro,
}

impl RandomSequence {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            rng: Xoroshiro::from_seed(seed),
        }
    }

    pub fn random_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            return min;
        }
        self.rng.next_inbetween_i32(min, max)
    }
}

/// Persistent/runtime manager for server random sequences.
pub struct RandomSequences {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: HashMap<String, RandomSequence>,
}

impl Default for RandomSequences {
    fn default() -> Self {
        Self::new()
    }
}

impl RandomSequences {
    #[must_use]
    pub fn new() -> Self {
        Self {
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: HashMap::new(),
        }
    }

    fn calculate_seed(
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> u64 {
        let base_seed = if include_world_seed { world_seed } else { 0 };
        let mut seed = (base_seed ^ i64::from(salt)) as u64;
        if include_sequence_id {
            let mut hasher = DefaultHasher::new();
            hasher.write(sequence.to_string().as_bytes());
            seed ^= hasher.finish();
        }
        seed
    }

    pub fn get_or_create(&mut self, sequence: &Identifier, world_seed: i64) -> &mut RandomSequence {
        let key = sequence.to_string();
        let salt = self.salt;
        let include_world_seed = self.include_world_seed;
        let include_sequence_id = self.include_sequence_id;
        self.sequences.entry(key).or_insert_with(|| {
            let seed = Self::calculate_seed(
                sequence,
                world_seed,
                salt,
                include_world_seed,
                include_sequence_id,
            );
            RandomSequence::new(seed)
        })
    }

    pub fn reset(&mut self, sequence: &Identifier, world_seed: i64) {
        let key = sequence.to_string();
        let seed = Self::calculate_seed(
            sequence,
            world_seed,
            self.salt,
            self.include_world_seed,
            self.include_sequence_id,
        );
        self.sequences.insert(key, RandomSequence::new(seed));
    }

    pub fn reset_with_options(
        &mut self,
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        let key = sequence.to_string();
        let seed = Self::calculate_seed(
            sequence,
            world_seed,
            salt,
            include_world_seed,
            include_sequence_id,
        );
        self.sequences.insert(key, RandomSequence::new(seed));
    }

    pub fn clear(&mut self) -> usize {
        let count = self.sequences.len();
        self.sequences.clear();
        count
    }

    pub const fn set_seed_defaults(
        &mut self,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.salt = salt;
        self.include_world_seed = include_world_seed;
        self.include_sequence_id = include_sequence_id;
    }

    #[must_use]
    pub fn get_sequence_keys(&self) -> Vec<String> {
        self.sequences.keys().cloned().collect()
    }

    /// The `data` compound of vanilla's `random_sequences.dat` (`RandomSequences.CODEC`).
    #[must_use]
    #[expect(clippy::cast_possible_wrap)]
    pub fn to_nbt(&self) -> NbtCompound {
        let mut data = NbtCompound::new();
        data.put_int("salt", self.salt);
        // Both flags are `optionalFieldOf(.., true)`, which leaves out the default.
        if !self.include_world_seed {
            data.put_bool("include_world_seed", false);
        }
        if !self.include_sequence_id {
            data.put_bool("include_sequence_id", false);
        }
        let mut sequences = NbtCompound::new();
        for (id, sequence) in &self.sequences {
            let (lo, hi) = sequence.rng.state();
            let mut tag = NbtCompound::new();
            tag.put("source", NbtTag::LongArray(vec![lo as i64, hi as i64]));
            sequences.put_compound(id, tag);
        }
        data.put_compound("sequences", sequences);
        data
    }

    /// Reads the `data` compound of vanilla's `random_sequences.dat`.
    #[must_use]
    #[expect(clippy::cast_sign_loss)]
    pub fn from_nbt(data: &NbtCompound) -> Self {
        let mut result = Self::new();
        result.salt = data.get_int("salt").unwrap_or(0);
        result.include_world_seed = data.get_bool("include_world_seed").unwrap_or(true);
        result.include_sequence_id = data.get_bool("include_sequence_id").unwrap_or(true);
        if let Some(sequences) = data.get_compound("sequences") {
            for (id, tag) in &sequences.child_tags {
                let NbtTag::Compound(sequence) = tag else {
                    continue;
                };
                if let Some(&[lo, hi]) = sequence.get_long_array("source") {
                    result.sequences.insert(
                        id.to_string(),
                        RandomSequence {
                            rng: Xoroshiro::from_state(lo as u64, hi as u64),
                        },
                    );
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `testdata/random_sequences_vanilla_26_3.dat` is what vanilla 26.3 saved after two
    /// `/random value 1..1000000 test:seq`; the next value it gave after that save was 848662.
    #[test]
    fn a_vanilla_sequence_continues_where_vanilla_left_it() {
        let root = pumpkin_nbt::nbt_compress::read_gzip_compound_tag(std::io::Cursor::new(
            include_bytes!("testdata/random_sequences_vanilla_26_3.dat"),
        ))
        .unwrap();
        let data = root.get_compound("data").unwrap();
        let mut sequences = RandomSequences::from_nbt(data);
        assert_eq!(sequences.to_nbt(), *data);
        let sequence = sequences.sequences.get_mut("test:seq").unwrap();
        assert_eq!(sequence.random_between_inclusive(1, 1_000_000), 848_662);
    }
}
