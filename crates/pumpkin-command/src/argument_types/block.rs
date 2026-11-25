use crate::{
    argument_types::argument_type::{ArgumentType, JavaClientArgumentType},
    argument_types::nbt::NbtCompoundArgumentType,
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};
use pumpkin_data::{Block, BlockStateId, translation};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::text::TextComponent;

pub const INVALID_BLOCK_ERROR_TYPE: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
    translation::java::ARGUMENT_BLOCK_ID_INVALID,
);

/// Arguments: the block, then the property.
pub const ERROR_UNKNOWN_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
);

/// Arguments: the property, then the block.
pub const ERROR_DUPLICATE_PROPERTY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
);

/// Arguments: the block, the value, then the property.
pub const ERROR_INVALID_VALUE: CommandErrorType<3> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
    translation::java::ARGUMENT_BLOCK_PROPERTY_INVALID,
);

/// Arguments: the property, then the block.
pub const ERROR_EXPECTED_VALUE: CommandErrorType<2> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
    translation::java::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
);

pub const ERROR_EXPECTED_END_OF_PROPERTIES: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
    translation::java::ARGUMENT_BLOCK_PROPERTY_UNCLOSED,
);

/// A parsed `block_state` argument, like vanilla's `BlockInput`: the block state, the
/// properties the input named, and the block entity data to load into the placed block.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockInput {
    pub block: &'static Block,
    pub state: BlockStateId,
    pub defined_properties: Vec<&'static str>,
    pub nbt: Option<NbtCompound>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BlockArgumentType;

/// The block's property names in state order, each with every value it takes.
fn property_values(block: &'static Block) -> Vec<(&'static str, Vec<&'static str>)> {
    let mut names: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for state in block.states {
        let Some(props) = block.properties(state.id) else {
            break;
        };
        for prop in props.to_props() {
            let name = prop.key().to_str();
            let value = prop.value_str();
            match names.iter_mut().find(|(n, _)| *n == name) {
                Some((_, values)) => {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                }
                None => names.push((name, vec![value])),
            }
        }
    }
    names
}

/// The default state of `block` with the `chosen` properties set.
fn state_with(block: &'static Block, chosen: &[(&'static str, &'static str)]) -> BlockStateId {
    let default = block.default_state.id;
    let Some(default_props) = block.properties(default).map(|props| props.to_props()) else {
        return default;
    };
    block
        .states
        .iter()
        .find(|state| {
            block.properties(state.id).is_some_and(|props| {
                props
                    .to_props()
                    .iter()
                    .zip(&default_props)
                    .all(|(prop, default_prop)| {
                        let name = prop.key().to_str();
                        chosen
                            .iter()
                            .find(|(n, _)| *n == name)
                            .map_or(prop == default_prop, |(_, value)| {
                                prop.value_str() == *value
                            })
                    })
            })
        })
        .map_or(default, |state| state.id)
}

/// Reads `[key=value,...]` like vanilla `BlockStateParser.readProperties`.
pub(crate) fn read_properties(
    reader: &mut StringReader,
    block: &'static Block,
    id: &str,
) -> Result<Vec<(&'static str, &'static str)>, CommandSyntaxError> {
    let properties = property_values(block);
    let mut chosen: Vec<(&'static str, &'static str)> = Vec::new();
    reader.skip();
    reader.skip_whitespace();
    while reader.can_read_char() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let key_start = reader.cursor();
        let key = reader.read_string()?;
        let Some((name, values)) = properties.iter().find(|(name, _)| *name == key) else {
            reader.set_cursor(key_start);
            return Err(ERROR_UNKNOWN_PROPERTY.create(
                reader,
                TextComponent::text(id.to_string()),
                TextComponent::text(key),
            ));
        };
        if chosen.iter().any(|(n, _)| n == name) {
            reader.set_cursor(key_start);
            return Err(ERROR_DUPLICATE_PROPERTY.create(
                reader,
                TextComponent::text(key),
                TextComponent::text(id.to_string()),
            ));
        }
        reader.skip_whitespace();
        if reader.peek() != Some('=') {
            return Err(ERROR_EXPECTED_VALUE.create(
                reader,
                TextComponent::text(key),
                TextComponent::text(id.to_string()),
            ));
        }
        reader.skip();
        reader.skip_whitespace();
        let value_start = reader.cursor();
        let raw = reader.read_string()?;
        let Some(value) = values.iter().find(|value| **value == raw) else {
            reader.set_cursor(value_start);
            return Err(ERROR_INVALID_VALUE.create(
                reader,
                TextComponent::text(id.to_string()),
                TextComponent::text(raw),
                TextComponent::text(key),
            ));
        };
        chosen.push((name, value));
        reader.skip_whitespace();
        if reader.can_read_char() {
            if reader.peek() != Some(',') {
                if reader.peek() != Some(']') {
                    return Err(ERROR_EXPECTED_END_OF_PROPERTIES.create(reader));
                }
                break;
            }
            reader.skip();
        }
    }
    if reader.can_read_char() {
        reader.skip();
        Ok(chosen)
    } else {
        Err(ERROR_EXPECTED_END_OF_PROPERTIES.create(reader))
    }
}

impl<S: crate::source::CommandSource> ArgumentType<S> for BlockArgumentType {
    type Item = BlockInput;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let start = reader.cursor();
        while let Some(c) = reader.peek() {
            if c.is_alphanumeric() || c == '_' || c == ':' || c == '/' || c == '.' || c == '-' {
                reader.skip();
            } else {
                break;
            }
        }
        let block_name = &reader.string()[start..reader.cursor()];
        let normalized = if block_name.contains(':') {
            block_name.to_string()
        } else {
            format!("minecraft:{block_name}")
        };

        let Some(block) = Block::from_name(&normalized) else {
            return Err(INVALID_BLOCK_ERROR_TYPE.create(reader, TextComponent::text(normalized)));
        };

        let (state, defined_properties) = if reader.peek() == Some('[') {
            let chosen = read_properties(reader, block, &normalized)?;
            let state = state_with(block, &chosen);
            (state, chosen.into_iter().map(|(name, _)| name).collect())
        } else {
            (block.default_state.id, Vec::new())
        };

        let nbt = if reader.peek() == Some('{') {
            Some(ArgumentType::<crate::source::DummySource>::parse(
                &NbtCompoundArgumentType,
                reader,
            )?)
        } else {
            None
        };

        Ok(BlockInput {
            block,
            state,
            defined_properties,
            nbt,
        })
    }

    fn client_side_parser(&'_ self) -> JavaClientArgumentType {
        JavaClientArgumentType::BlockState
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext<S>,
        builder: SuggestionsBuilder,
    ) -> Suggestions {
        builder.build()
    }
}

impl BlockArgumentType {
    /// The parsed block, without the state or block entity data the input gave.
    pub fn get<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<&'static Block, CommandSyntaxError> {
        Ok(context.get_argument::<BlockInput>(name)?.block)
    }

    /// The parsed block state, its named properties and its block entity data.
    pub fn get_input<S: crate::source::CommandSource>(
        context: &CommandContext<S>,
        name: &str,
    ) -> Result<BlockInput, CommandSyntaxError> {
        Ok(context.get_argument::<BlockInput>(name)?.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::errors::error_types::AnyCommandErrorType;
    use crate::source::DummySource;

    fn parse(input: &str) -> Result<BlockInput, CommandSyntaxError> {
        ArgumentType::<DummySource>::parse(&BlockArgumentType, &mut StringReader::new(input))
    }

    fn value(input: &BlockInput, name: &str) -> &'static str {
        input
            .block
            .properties(input.state)
            .expect("block has properties")
            .to_props()
            .into_iter()
            .find(|prop| prop.key().to_str() == name)
            .map(pumpkin_data::Property::value_str)
            .expect("property exists")
    }

    /// The error type and the cursor the error points at.
    fn error(input: &str) -> (&'static dyn AnyCommandErrorType, usize) {
        let err = parse(input).expect_err("input should not parse");
        (
            err.error_type,
            err.context.expect("error has context").cursor,
        )
    }

    #[test]
    fn plain_block_is_the_default_state() {
        let input = parse("stone").unwrap();
        assert_eq!(input.state, Block::STONE.default_state.id);
        assert!(input.defined_properties.is_empty());
        assert!(input.nbt.is_none());
        let input = parse("minecraft:piston[]").unwrap();
        assert_eq!(input.state, Block::PISTON.default_state.id);
    }

    #[test]
    fn properties_pick_the_state() {
        let input = parse("minecraft:piston[facing=east]").unwrap();
        assert_eq!(input.block, &Block::PISTON);
        assert_eq!(value(&input, "facing"), "east");
        assert_eq!(value(&input, "extended"), "false");
        assert_eq!(input.defined_properties, vec!["facing"]);

        let input = parse("oak_stairs[ half = top , facing=south,waterlogged=true ]").unwrap();
        assert_eq!(value(&input, "half"), "top");
        assert_eq!(value(&input, "facing"), "south");
        assert_eq!(value(&input, "waterlogged"), "true");
        assert_eq!(value(&input, "shape"), "straight");

        let input = parse("repeater[delay=3,locked=true]").unwrap();
        assert_eq!(value(&input, "delay"), "3");
        assert_eq!(value(&input, "locked"), "true");
        assert_eq!(value(&input, "facing"), "north");
    }

    #[test]
    fn block_entity_data_follows_the_state() {
        let input =
            parse("minecraft:chest[facing=west]{Items:[{Slot:0b,id:\"minecraft:stone\",count:3}]}")
                .unwrap();
        assert_eq!(value(&input, "facing"), "west");
        let nbt = input.nbt.expect("chest data parsed");
        assert_eq!(nbt.get_list("Items").map(<[_]>::len), Some(1));
        let input = parse("chest{CustomName:\"Box\"}").unwrap();
        assert_eq!(input.state, Block::CHEST.default_state.id);
        assert!(input.nbt.is_some());
    }

    #[test]
    fn bad_properties_fail_like_vanilla() {
        let unknown: &'static dyn AnyCommandErrorType = &ERROR_UNKNOWN_PROPERTY;
        let invalid: &'static dyn AnyCommandErrorType = &ERROR_INVALID_VALUE;
        let duplicate: &'static dyn AnyCommandErrorType = &ERROR_DUPLICATE_PROPERTY;
        let no_value: &'static dyn AnyCommandErrorType = &ERROR_EXPECTED_VALUE;
        let unclosed: &'static dyn AnyCommandErrorType = &ERROR_EXPECTED_END_OF_PROPERTIES;

        assert_eq!(error("piston[foo=bar]"), (unknown, 7));
        assert_eq!(error("stone[facing=east]"), (unknown, 6));
        assert_eq!(error("piston[facing=sideways]"), (invalid, 14));
        assert_eq!(error("chest[facing=up]"), (invalid, 13));
        assert_eq!(error("piston[facing=east,facing=up]"), (duplicate, 19));
        assert_eq!(error("piston[facing]").0, no_value);
        assert_eq!(error("piston[facing=east").0, unclosed);
        assert_eq!(error("piston[facing=east;extended=true]").0, unclosed);
    }
}
