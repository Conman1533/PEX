// Copyright (c) 2026 NicDevTV
// SPDX-License-Identifier: MIT

use crate::engine::{BlockPos, position_hash};
use pumpkin_data::{Block, BlockStateId};

#[derive(Clone, Debug)]
pub struct BlockPattern {
    choices: Vec<WeightedBlock>,
    total_weight: u32,
}

#[derive(Clone, Copy, Debug)]
struct WeightedBlock {
    state: u16,
    weight: u32,
}

impl BlockPattern {
    /// Chooses a state by weight, deterministically for the given position and edit seed.
    pub(crate) fn choose(&self, pos: BlockPos, seed: u64) -> u16 {
        let mut cursor = position_hash(pos, seed) % u64::from(self.total_weight);
        for choice in &self.choices {
            if cursor < u64::from(choice.weight) {
                return choice.state;
            }
            cursor -= u64::from(choice.weight);
        }
        self.choices
            .last()
            .map(|choice| choice.state)
            .unwrap_or_default()
    }
}

trait BlockRegistry {
    /// Resolves a block name and property overrides to a registered state ID.
    fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16>;
    /// Returns the property name/value pairs for a state ID.
    fn properties(&self, state: u16) -> Vec<(String, String)>;
    /// Reports whether a numeric state ID exists in the registry.
    fn contains(&self, state: u16) -> bool;
}

struct ServerRegistry;

impl BlockRegistry for ServerRegistry {
    /// Resolves a state through Pumpkin's runtime block registry.
    fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16> {
        resolve_block_state(name, properties)
    }

    /// Reads a state's properties from Pumpkin's runtime registry.
    fn properties(&self, state: u16) -> Vec<(String, String)> {
        block_properties(state)
    }

    /// Checks whether Pumpkin recognizes the numeric state ID.
    fn contains(&self, state: u16) -> bool {
        BlockStateId::new(state).is_some()
    }
}

/// Resolves a block name and property overrides the way the Wasm host's `resolve-block-state` did.
fn resolve_block_state(name: &str, properties: &[(String, String)]) -> Option<u16> {
    let block = Block::from_name(name)?;
    if properties.is_empty() {
        return Some(block.default_state.id.as_u16());
    }
    let props: Vec<(&str, &str)> = properties
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    // `from_properties` panics on unknown property values.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        block.from_properties(&props).to_state_id(block).as_u16()
    }))
    .ok()
}

fn block_properties(state: u16) -> Vec<(String, String)> {
    let state = BlockStateId::new_or_air(state);
    Block::from_state_id(state)
        .properties(state)
        .map(|props| {
            props
                .to_props()
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Reports whether placing this state creates a block entity, like `block_entity_type != u16::MAX`.
pub fn has_block_entity(state: u16) -> bool {
    BlockStateId::new(state).is_some_and(|state| state.to_state().block_entity_type != u16::MAX)
}

/// Parses a registered numeric ID or block name with optional `[key=value,...]` properties.
///
/// Returns an error for unknown states, malformed syntax, or properties the host ignores.
pub fn parse_block_state(input: &str) -> Result<u16, String> {
    resolve_state(input, &ServerRegistry)
}

/// Resolves a state against the supplied registry and verifies every requested property.
fn resolve_state(input: &str, registry: &impl BlockRegistry) -> Result<u16, String> {
    let input = input.trim();
    if let Ok(state) = input.parse::<u16>() {
        return registry
            .contains(state)
            .then_some(state)
            .ok_or_else(|| format!("unknown block state ID `{state}`"));
    }

    let ParsedBlock { name, properties } = parse_state_parts(input)?;
    let state = registry
        .resolve(name, &properties)
        .ok_or_else(|| format!("unknown block state `{input}`"))?;
    // Pumpkin ignores unknown property names; reject them rather than editing the wrong state.
    if !properties.is_empty() {
        let resolved = registry.properties(state);
        if properties
            .iter()
            .any(|property| !resolved.contains(property))
        {
            return Err(format!("invalid block properties in `{input}`"));
        }
    }
    Ok(state)
}

struct ParsedBlock<'a> {
    name: &'a str,
    properties: Vec<(String, String)>,
}

/// Splits a block name from its properties, rejecting malformed or duplicate properties.
fn parse_state_parts(input: &str) -> Result<ParsedBlock<'_>, String> {
    let (name, properties) = match input.split_once('[') {
        Some((name, suffix)) => {
            let properties = suffix
                .strip_suffix(']')
                .ok_or_else(|| "missing closing `]` in block state".to_owned())?;
            (name.trim(), Some(properties))
        }
        None => (input, None),
    };
    if name.is_empty() || name.contains(['[', ']', ',']) {
        return Err("invalid block name".to_owned());
    }
    let mut result: Vec<(String, String)> = Vec::new();
    if let Some(properties) = properties.filter(|properties| !properties.trim().is_empty()) {
        for property in properties.split(',') {
            let (key, value) = property
                .split_once('=')
                .ok_or_else(|| format!("invalid block property `{property}`"))?;
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty()
                || value.is_empty()
                || key.contains(['[', ']'])
                || value.contains(['[', ']', '='])
                || result.iter().any(|(existing, _)| existing == key)
            {
                return Err(format!("invalid or duplicate block property `{property}`"));
            }
            result.push((key.to_owned(), value.to_owned()));
        }
    }
    Ok(ParsedBlock {
        name,
        properties: result,
    })
}

/// Parses comma-separated blocks with optional positive `weight%` prefixes.
///
/// Property commas stay within their block; omitted weights default to one.
/// Returns an error for invalid states, syntax, zero weights, or a total weight overflow.
pub fn parse_block_pattern(input: &str) -> Result<BlockPattern, String> {
    resolve_pattern(input, &ServerRegistry)
}

/// Resolves each weighted pattern choice and checks that the total weight fits in `u32`.
fn resolve_pattern(input: &str, registry: &impl BlockRegistry) -> Result<BlockPattern, String> {
    let mut choices = Vec::new();
    let mut total_weight = 0u32;
    for part in pattern_tokens(input)? {
        let part = part.trim();
        if part.is_empty() {
            return Err("empty block in pattern".to_owned());
        }
        let (weight, block) = parse_weighted_block(part)?;
        let state = resolve_state(block, registry)?;
        total_weight = total_weight
            .checked_add(weight)
            .ok_or_else(|| "pattern weights are too large".to_owned())?;
        choices.push(WeightedBlock { state, weight });
    }
    Ok(BlockPattern {
        choices,
        total_weight,
    })
}

/// Splits a pattern at commas outside property brackets, rejecting unbalanced brackets.
fn pattern_tokens(input: &str) -> Result<Vec<&str>, String> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut in_properties = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' if !in_properties => in_properties = true,
            ']' if in_properties => in_properties = false,
            '[' | ']' => return Err("invalid brackets in block pattern".to_owned()),
            ',' if !in_properties => {
                tokens.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if in_properties {
        return Err("missing closing `]` in block pattern".to_owned());
    }
    tokens.push(&input[start..]);
    Ok(tokens)
}

/// Parses a positive `weight%block` pair, using weight one when no prefix is present.
fn parse_weighted_block(input: &str) -> Result<(u32, &str), String> {
    let Some((weight, block)) = input.split_once('%') else {
        return Ok((1, input));
    };
    let weight = weight
        .trim()
        .parse::<u32>()
        .map_err(|err| format!("invalid pattern weight `{}`: {err}", weight.trim()))?;
    if weight == 0 {
        return Err("pattern weights must be greater than 0".to_owned());
    }
    let block = block.trim();
    if block.is_empty() {
        return Err("missing block after pattern weight".to_owned());
    }
    Ok((weight, block))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Registry;
    impl BlockRegistry for Registry {
        /// Resolves the test blocks, including the top and bottom stair variants.
        fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<u16> {
            match name.strip_prefix("minecraft:").unwrap_or(name) {
                "stone" => Some(1),
                "dirt" => Some(2),
                "stairs"
                    if properties
                        .iter()
                        .any(|(key, value)| key == "half" && value == "top") =>
                {
                    Some(4)
                }
                "stairs" => Some(3),
                _ => None,
            }
        }
        /// Returns the fixture properties used to detect invalid stair overrides.
        fn properties(&self, state: u16) -> Vec<(String, String)> {
            match state {
                3 => vec![
                    ("facing".into(), "north".into()),
                    ("half".into(), "bottom".into()),
                ],
                4 => vec![
                    ("facing".into(), "north".into()),
                    ("half".into(), "top".into()),
                ],
                _ => vec![],
            }
        }
        /// Accepts only the four numeric state IDs defined by the test registry.
        fn contains(&self, state: u16) -> bool {
            (1..=4).contains(&state)
        }
    }

    /// Checks that names and registered numeric IDs resolve and unknown states fail.
    #[test]
    fn resolves_names_and_valid_numeric_ids() {
        assert_eq!(resolve_state("minecraft:stone", &Registry).unwrap(), 1);
        assert_eq!(resolve_state("2", &Registry).unwrap(), 2);
        assert!(resolve_state("65535", &Registry).is_err());
        assert!(resolve_state("missing", &Registry).is_err());
    }

    /// Checks that property order is irrelevant and omitted properties retain host defaults.
    #[test]
    fn resolves_properties_in_any_order_and_keeps_defaults() {
        assert_eq!(
            resolve_state("stairs[half=top,facing=north]", &Registry).unwrap(),
            4
        );
        assert_eq!(
            resolve_state("stairs[facing=north,half=top]", &Registry).unwrap(),
            4
        );
        assert_eq!(resolve_state("stairs[half=top]", &Registry).unwrap(), 4);
    }

    /// Checks that malformed properties and overrides ignored by the host are rejected.
    #[test]
    fn rejects_properties_the_host_ignores() {
        for input in [
            "stone[foo=bar]",
            "stairs[facing=invalid]",
            "stairs[half=top,half=bottom]",
            "stairs[half=top",
            "stone]",
            "stairs[half=top]=",
        ] {
            assert!(resolve_state(input, &Registry).is_err(), "{input}");
        }
    }

    /// Checks that property commas do not split weighted pattern choices.
    #[test]
    fn weighted_patterns_keep_property_commas_together() {
        let pattern =
            resolve_pattern("50%stairs[half=top,facing=north],50%dirt", &Registry).unwrap();
        assert_eq!(pattern.total_weight, 100);
        assert_eq!(pattern.choices.len(), 2);
        assert_eq!(pattern.choices[0].state, 4);
    }

    /// Checks rejection of empty choices, zero or overflowing weights, and nested brackets.
    #[test]
    fn rejects_empty_zero_weight_and_overflowing_patterns() {
        for input in [
            "",
            "stone,",
            ",stone",
            "0%stone",
            "4294967295%stone,1%dirt",
            "stairs[[half=top]]",
        ] {
            assert!(resolve_pattern(input, &Registry).is_err(), "{input}");
        }
    }

    /// Checks that an unweighted single-block pattern selects its only state.
    #[test]
    fn single_block_patterns_keep_the_same_state() {
        let pattern = resolve_pattern("stone", &Registry).unwrap();
        assert_eq!(pattern.total_weight, 1);
        assert_eq!(pattern.choose(BlockPos { x: 0, y: 0, z: 0 }, 1), 1);
    }
}
