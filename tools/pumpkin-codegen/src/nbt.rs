//! NBT writers for byte literals in generated code.
//!
//! `NbtCompound` stores its tags in a `HashMap`, so `pumpkin_nbt`'s own writers put compound
//! keys in a different order on every run. These write the same encoding with the keys sorted.

use pumpkin_nbt::{
    COMPOUND_ID, END_ID,
    compound::NbtCompound,
    serializer::{NbtWriteHelper, NbtWriteHelperBedrock, NbtWriteHelperJava, Result},
    tag::NbtTag,
};

/// Same bytes as `Nbt::write_unnamed`, with compound keys sorted.
pub fn write_unnamed(compound: NbtCompound) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = NbtWriteHelperJava::new(&mut bytes);
    writer
        .write_u8(COMPOUND_ID)
        .and_then(|()| write_compound(compound, &mut writer))
        .expect("Failed to write NBT");
    bytes
}

/// Same bytes as `Nbt::write_bedrock` with an empty root name, with compound keys sorted.
pub fn write_bedrock(compound: NbtCompound) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = NbtWriteHelperBedrock::new(&mut bytes);
    writer
        .write_u8(COMPOUND_ID)
        .and_then(|()| writer.write_string(""))
        .and_then(|()| write_compound(compound, &mut writer))
        .expect("Failed to write NBT");
    bytes
}

/// Same bytes as `NbtTag::serialize`, with compound keys sorted.
pub fn write_tag<W: NbtWriteHelper>(tag: NbtTag, writer: &mut W) -> Result<()> {
    writer.write_u8(tag.get_type_id())?;
    write_data(tag, writer)
}

fn write_compound<W: NbtWriteHelper>(compound: NbtCompound, writer: &mut W) -> Result<()> {
    let mut tags: Vec<_> = compound.child_tags.into_iter().collect();
    tags.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    for (name, tag) in tags {
        writer.write_u8(tag.get_type_id())?;
        writer.write_string(&name)?;
        write_data(tag, writer)?;
    }
    writer.write_u8(END_ID)
}

/// Mirrors `NbtTag::serialize_data`, including how a list of mixed tag types is written as a
/// list of compounds that wrap each element under the key `""`.
fn write_data<W: NbtWriteHelper>(tag: NbtTag, writer: &mut W) -> Result<()> {
    match tag {
        NbtTag::Compound(compound) => write_compound(compound, writer),
        NbtTag::List(list) => {
            let mut element_id = END_ID;
            for element in &list {
                let id = element.get_type_id();
                if element_id == END_ID {
                    element_id = id;
                } else if element_id != id {
                    element_id = COMPOUND_ID;
                    break;
                }
            }

            writer.write_u8(element_id)?;
            writer.write_i32(i32::try_from(list.len()).expect("NBT list too long"))?;
            for element in list {
                let needs_wrapper = element_id == COMPOUND_ID
                    && match &element {
                        NbtTag::Compound(compound) => {
                            compound.child_tags.len() == 1 && compound.child_tags.contains_key("")
                        }
                        _ => true,
                    };
                if needs_wrapper {
                    let mut wrapper = NbtCompound::new();
                    wrapper.put("", element);
                    write_compound(wrapper, writer)?;
                } else {
                    write_data(element, writer)?;
                }
            }
            Ok(())
        }
        tag => tag.serialize_data(writer),
    }
}
