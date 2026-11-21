use super::BlockEntity;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use std::sync::Mutex;

pub struct DecoratedPotBlockEntity {
    pub position: BlockPos,
    pub sherds: Mutex<Option<Vec<NbtTag>>>,
    pub item: Mutex<Option<ItemStack>>,
}

impl BlockEntity for DecoratedPotBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let sherds = read_sherds(nbt);
        let item = nbt
            .get_compound("item")
            .and_then(ItemStack::read_item_stack);
        Self {
            position,
            sherds: Mutex::new(sherds),
            item: Mutex::new(item),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Ok(sherds) = self.sherds.lock()
            && let Some(sh) = sherds.as_ref()
        {
            nbt.put_compound("sherds", sherds_compound(sh));
        }
        if let Ok(item) = self.item.lock()
            && let Some(it) = item.as_ref()
        {
            let mut it_nbt = NbtCompound::new();
            it.write_item_stack(&mut it_nbt);
            nbt.put_compound("item", it_nbt);
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        if let Ok(sherds) = self.sherds.try_lock()
            && let Some(ref sh) = *sherds
        {
            nbt.put_compound("sherds", sherds_compound(sh));
        }
        if let Ok(item) = self.item.try_lock()
            && let Some(ref it) = *item
        {
            let mut it_nbt = NbtCompound::new();
            it.write_item_stack(&mut it_nbt);
            nbt.put_compound("item", it_nbt);
        }
        Some(nbt)
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let sherds_guard = self.sherds.lock().ok();
        let sherds = sherds_guard.as_ref().and_then(|s| s.as_deref());
        Some(crate::block::entities::bedrock::build_decorated_pot_actor(
            self.position,
            sherds,
        ))
    }

    fn collect_item_components(&self, stack: &mut ItemStack) {
        if let Ok(sherds) = self.sherds.lock()
            && let Some(sh) = sherds.as_ref()
        {
            let sherds_vec: Vec<std::borrow::Cow<'static, str>> = sh
                .iter()
                .filter_map(NbtTag::extract_string)
                .map(|s| std::borrow::Cow::Owned(s.to_string()))
                .collect();
            if !sherds_vec.is_empty() {
                stack.set_data_component(pumpkin_data::data_component_impl::PotDecorationsImpl {
                    sherds: sherds_vec,
                });
            }
        }
        if let Ok(item) = self.item.lock()
            && let Some(it) = item.as_ref()
            && !it.is_empty()
        {
            stack.set_data_component(pumpkin_data::data_component_impl::ContainerImpl {
                items: vec![(0, it.clone())],
            });
        }
    }

    fn apply_item_components(&self, stack: &ItemStack) {
        if let Some(decorations) =
            stack.get_data_component::<pumpkin_data::data_component_impl::PotDecorationsImpl>()
            && !decorations.sherds.is_empty()
        {
            let tags: Vec<NbtTag> = decorations
                .sherds
                .iter()
                .map(|s| NbtTag::String(s.to_string().into()))
                .collect();
            *self
                .sherds
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tags);
        }
        if let Some(container) =
            stack.get_data_component::<pumpkin_data::data_component_impl::ContainerImpl>()
            && let Some((_, it)) = container.items.first()
            && !it.is_empty()
        {
            *self
                .item
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(it.clone());
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The sides of vanilla `PotDecorations`, in the order of its item component's list.
const POT_SIDES: [&str; 4] = ["back", "left", "right", "front"];

/// The pot's sherds as item ids in `POT_SIDES` order, from vanilla's `sherds` compound (each side an
/// item stack template) or the list PEX saved before.
fn read_sherds(nbt: &NbtCompound) -> Option<Vec<NbtTag>> {
    if let Some(list) = nbt.get_list("sherds") {
        return Some(list.to_vec());
    }
    let sides = nbt.get_compound("sherds")?;
    let mut any = false;
    let sherds = POT_SIDES
        .iter()
        .map(|side| {
            let id = sides
                .get_compound(side)
                .and_then(|template| template.get_string("id"));
            any |= id.is_some();
            NbtTag::String(id.unwrap_or("minecraft:brick").into())
        })
        .collect();
    any.then_some(sherds)
}

/// Vanilla `PotDecorations.CODEC`: each side as an item stack template.
fn sherds_compound(sherds: &[NbtTag]) -> NbtCompound {
    let mut sides = NbtCompound::new();
    for (side, sherd) in POT_SIDES.iter().zip(sherds) {
        if let Some(id) = sherd.extract_string() {
            let mut template = NbtCompound::new();
            template.put_string("id", id.to_string());
            sides.put_compound(side, template);
        }
    }
    sides
}

impl DecoratedPotBlockEntity {
    pub const ID: &'static str = "minecraft:decorated_pot";

    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            sherds: Mutex::new(None),
            item: Mutex::new(None),
        }
    }

    pub fn get_item(&self) -> Option<ItemStack> {
        self.item
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn take_item(&self) -> Option<ItemStack> {
        self.item
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    pub fn try_insert_item(&self, stack: &mut ItemStack, count: u8) -> bool {
        let mut item_guard = self
            .item
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = item_guard.as_mut() {
            if existing.item.id == stack.item.id {
                // Vanilla gates on the item's own max stack size, not a fixed 64.
                // Saturate so a count loaded from NBT above that limit cannot underflow.
                let space = existing
                    .get_max_stack_size()
                    .saturating_sub(existing.item_count);
                let add = count.min(space).min(stack.item_count);
                if add > 0 {
                    existing.item_count += add;
                    stack.item_count -= add;
                    return true;
                }
            }
            false
        } else {
            let insert_count = count.min(stack.item_count).min(stack.get_max_stack_size());
            let mut inserted = stack.clone();
            inserted.item_count = insert_count;
            *item_guard = Some(inserted);
            stack.item_count -= insert_count;
            true
        }
    }

    pub fn get_comparator_output(&self) -> u8 {
        self.item
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or(0, |item| {
                if item.item_count == 0 {
                    0
                } else {
                    // Vanilla scales by the item's own stack size, not a fixed 64.
                    // `try_insert_item` may hold more than one stack, so cap the ratio if full.
                    let max_count = f32::from(item.get_max_stack_size());
                    let filled = (f32::from(item.item_count) / max_count).min(1.0);
                    1 + (filled * 14.0).floor() as u8
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sherds_save_as_vanilla_sides_and_read_back() {
        let sherds: Vec<NbtTag> = [
            "minecraft:brick",
            "minecraft:heart_pottery_sherd",
            "minecraft:brick",
            "minecraft:skull_pottery_sherd",
        ]
        .iter()
        .map(|id| NbtTag::String((*id).into()))
        .collect();
        let pot = DecoratedPotBlockEntity::new(BlockPos::new(0, 0, 0));
        *pot.sherds.lock().expect("lock") = Some(sherds.clone());
        let mut nbt = NbtCompound::new();
        pot.write_nbt(&mut nbt);

        let sides = nbt.get_compound("sherds").expect("a sherds compound");
        assert_eq!(
            sides.get_compound("left").and_then(|t| t.get_string("id")),
            Some("minecraft:heart_pottery_sherd")
        );
        assert_eq!(
            sides.get_compound("front").and_then(|t| t.get_string("id")),
            Some("minecraft:skull_pottery_sherd")
        );
        assert_eq!(read_sherds(&nbt), Some(sherds));
    }
}
