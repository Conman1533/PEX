use std::collections::BTreeMap;
use std::fs;

use heck::ToPascalCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use serde::Deserialize;

#[derive(Deserialize)]
struct SoundTypeEntry {
    volume: f32,
    pitch: f32,
    #[serde(rename = "break")]
    break_sound: String,
    step: String,
    place: String,
    hit: String,
    fall: String,
}

#[derive(Deserialize)]
struct BlockEntry {
    #[serde(rename = "type")]
    sound_type: u8,
    #[serde(default)]
    states: BTreeMap<u16, u8>,
}

#[derive(Deserialize)]
struct BlockSounds {
    types: Vec<SoundTypeEntry>,
    blocks: BTreeMap<String, BlockEntry>,
}

#[derive(Deserialize)]
struct BlockName {
    id: u16,
    name: String,
}

#[derive(Deserialize)]
struct Blocks {
    blocks: Vec<BlockName>,
}

fn sound(name: &str) -> TokenStream {
    let variant = format_ident!("{}", name.to_pascal_case());
    quote! { Sound::#variant }
}

/// Generates vanilla `SoundType` (`block_sounds.json`): the sound set of every block and the
/// few block states that differ from their block.
///
/// `block_sounds.json` is dumped from the vanilla 26.3 jar (`BlockState.getSoundType` for every
/// state) until the Extractor exports sound types.
pub fn build() -> TokenStream {
    let sounds: BlockSounds =
        serde_json::from_str(&fs::read_to_string("../../assets/block_sounds.json").unwrap())
            .expect("Failed to parse block_sounds.json");
    let blocks: Blocks =
        serde_json::from_str(&fs::read_to_string("../../assets/blocks.json").unwrap())
            .expect("Failed to parse blocks.json");

    let types = sounds.types.iter().map(|t| {
        let (volume, pitch) = (t.volume, t.pitch);
        let break_sound = sound(&t.break_sound);
        let step_sound = sound(&t.step);
        let place_sound = sound(&t.place);
        let hit_sound = sound(&t.hit);
        let fall_sound = sound(&t.fall);
        quote! {
            BlockSoundType {
                volume: #volume,
                pitch: #pitch,
                break_sound: #break_sound,
                step_sound: #step_sound,
                place_sound: #place_sound,
                hit_sound: #hit_sound,
                fall_sound: #fall_sound,
            },
        }
    });

    let mut by_block = vec![
        0u8;
        blocks
            .blocks
            .iter()
            .map(|b| b.id as usize + 1)
            .max()
            .unwrap_or(0)
    ];
    let mut state_arms = TokenStream::new();
    for block in &blocks.blocks {
        let entry = sounds
            .blocks
            .get(&block.name)
            .unwrap_or_else(|| panic!("block_sounds.json has no entry for {}", block.name));
        by_block[block.id as usize] = entry.sound_type;
        for (state, sound_type) in &entry.states {
            let index = *sound_type as usize;
            state_arms.extend(quote! { #state => #index, });
        }
    }
    let type_count = sounds.types.len();
    let block_count = by_block.len();

    quote! {
        use crate::sound::Sound;
        use crate::{BlockId, BlockStateId};

        /// Vanilla `SoundType`: the sounds a block makes when broken, walked on, placed, hit
        /// and fallen on, and their volume and pitch.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct BlockSoundType {
            pub volume: f32,
            pub pitch: f32,
            pub break_sound: Sound,
            pub step_sound: Sound,
            pub place_sound: Sound,
            pub hit_sound: Sound,
            pub fall_sound: Sound,
        }

        static BLOCK_SOUND_TYPES: [BlockSoundType; #type_count] = [ #(#types)* ];

        static BLOCK_SOUND_TYPE_BY_BLOCK: [u8; #block_count] = [ #(#by_block),* ];

        impl BlockSoundType {
            /// Vanilla `BlockState.getSoundType` for a block and one of its states.
            #[must_use]
            pub fn of(block: BlockId, state: BlockStateId) -> &'static Self {
                let index = match state.as_u16() {
                    #state_arms
                    _ => BLOCK_SOUND_TYPE_BY_BLOCK[block.as_u16() as usize] as usize,
                };
                &BLOCK_SOUND_TYPES[index]
            }
        }
    }
}
