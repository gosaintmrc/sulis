//  This file is part of Sulis, a turn based RPG written in Rust.
//  Copyright 2018 Jared Stephen
//
//  Sulis is free software: you can redistribute it and/or modify
//  it under the terms of the GNU General Public License as published by
//  the Free Software Foundation, either version 3 of the License, or
//  (at your option) any later version.
//
//  Sulis is distributed in the hope that it will be useful,
//  but WITHOUT ANY WARRANTY; without even the implied warranty of
//  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//  GNU General Public License for more details.
//
//  You should have received a copy of the GNU General Public License
//  along with Sulis.  If not, see <http://www.gnu.org/licenses/>

use std::str::FromStr;
use std::{self, f32, u32};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Error;

use rand::{self, Rng};
use rlua::{self, Lua, UserData, UserDataMethods};

use sulis_core::util::{invalid_data_error, ExtInt};
use sulis_core::config::Config;
use sulis_core::resource::ResourceSet;
use sulis_rules::{Attribute, AttackKind, DamageKind, Attack, HitKind};
use sulis_module::{ImageLayer, Faction, Actor, InventoryBuilder};
use {ActorState, EntityState, GameState, Location, area_feedback_text::ColorKind};
use {ai, animation::{self}, script::*, MOVE_TO_THRESHOLD};

/// Represents a single entity for Lua scripts.  Also can represent an invalid,
/// non-existant entity in some cases.  Many script functions pass a parent
/// which is a script entity, and often targets, which is a `ScriptEntitySet`
/// that a ScriptEntity can be extracted from.
///
/// # `state_end() -> AIState`
/// Returns the AI state telling the caller to end the AI turn.
/// ## Examples
/// ```lua
///   function ai_action(parent, state)
///     -- tell ai to end turn immediately
///     _G.state = game:state_end()
///   end
/// ```
///
/// # `state_wait(time: Int) -> AIState`
/// Returns the AI state telling the caller to wait for the specified
/// number of milliseconds (`time`), and then call the AI again.
/// See `state_end`
///
/// # `vis_dist() -> Float`
/// Returns the currently visibility distance for this entity (how
/// many tiles on the map it can see).  This is dependant on the
/// area that the entity is in.
///
/// # `add_levels(class: String, levels: Int)`
/// Adds the specified number of levels of the specified class to this entity
///
/// # `add_xp(amount: Int)`
/// Adds the specified `amount` of XP to the entity.  For adding XP to
/// the party, you generally want to use `game:add_party_xp(amount)`
/// instead.
///
/// # `add_to_party(show_portrait: Bool (Optional))`
/// Adds this entity to the player's party.  `show_portrait` is whether the entity
/// shows up in the portraits area of the UI.  Defaults to true.
///
/// # `remove_from_party()`
/// Removes this entity from the player's party
///
/// # `set_faction(faction: String)`
/// Sets this entity to the specified `faction`.  Valid factions are currently
/// `Hostile`, `Neutral`, or `Friendly`.  Hostiles will attack the player and
/// friendlies on sight, but will not engage neutrals.
///
/// # `set_flag(flag: String, value: String (Optional))`
/// Sets a `flag` to be stored on this entity.  This value will persist as part of the
/// save game and can be used to store custom state.  If the value is not specified,
/// sets the flag exists (for querying with `has_flag()`), but does not neccessarily
/// set a specific value.
///
/// # `add_num_flag(flag: String, value: Float)`
/// Sets the specified `flag` to a floating point numeric `value`.  This is for
/// convenience to avoid Lua needing to parse numbers from a string flag.
///
/// # `get_flag(flag: String) -> String`
/// Returns the value of the specified `flag` on this entity.  Returns the lua
/// value of `Nil` if the flag does not exist.
///
/// # `has_flag(flag: String) -> Bool`
/// Returns true if the specified `flag` is set to any value on this entity, false
/// otherwise
///
/// # `get_num_flag(flag: String) -> Int`
/// Returns the numeric value of this `flag` set on this entity, or 0.0 if it has
/// not been set.
///
/// # `clear_flag(flag: String)`
/// Clears the `flag` from this entity, as if it had never been set.  Works for both
/// numeric and standard flags.  If the flag had not previously been set, does nothing.
/// After this method, `has_flag(flag)` will return `false`.
///
/// # `is_valid() -> Bool`
/// Returns true if this ScriptEntity references a valid entity that can be queried and
/// acted on, false otherwise.
///
/// # `is_dead() -> Bool`
/// Returns true if this entity is dead (zero hit points), false otherwise.  Dead entities
/// cannot be currently interacted with in meaningful ways.
///
/// # `is_party_member() -> Bool`
/// Returns true if this entity is a member of the player's party (or if it is the player),
/// false otherwise.
///
/// # `use_ability(ability: ScriptAbility) -> Bool`
/// The parent entity attempts to use the `ability`.  Returns true if the ability use was
/// successful, false if it was not.  After activating, the script will often need to handle
/// a targeter (depending on the ability), using the methods on `ScriptInterface` (the `game`
/// object).
///
/// # `use_item(item: ScriptUsableItem) -> Bool`
/// Attempts to use the specified `item`.  Returns true if the item use was successful, false
/// if it was not.  See `use_ability`.
///
/// # `swap_weapons() -> Bool`
/// Attempts to swap weapons from the currently held weapon set to the alternate weapon slots.
/// Returns true if this is succesful, false if it is not.  The entity must have enough AP
/// to complete the action.
///
/// # `abilities() -> ScriptAbilitySet`
/// Returns the ScriptAbilitySet with all the abilities that this entity can potentially activate.
///
/// # `targets() -> ScriptEntitySet`
/// Creates a ScriptEntitySet consisting of all possible targets for abilities or items used
/// by this entity.  This includes all known entities in the same area as the parent entity.
///
/// # `remove_effects_with_tag(tag: String)`
/// Removes all currently active effects applied to this entity that have the specified tag.
///
/// # `create_effect(name: String, duration: Int (Optional)) -> ScriptEffect`
/// Creates a new effect with the specified `name` and `duration`.  If `duration` is not
/// specified, it is infinite, and will remain until removed or deactivated for a mode.
/// The effect will not be in effect until you call `apply()` on it.
///
/// # `create_surface(name: String, points: Table, duration: Int (Optional)) -> ScriptEffect`
/// Creates a surface effect with this entity as the parent.  This is a special case of
/// `create_effect`, above.  The effect must have `apply()`
/// called in order to actually be put into effect.  See `ScriptEffect`.
/// The `points` used by this method is a table of tables with `x` and `y` elements.  This
/// can be constructed by hand, or obtained from a `ScriptEntitySet` as the `affected_points`.
///
/// # `create_image_layer_anim(duration: Floag (Optional)) -> ScriptImageLayerAnimation`
/// Creates an image layer animation that will add (or override) image layers of the entity
/// for the specified duraiton.  If `duration` is not specified, the animation lasts forever
/// or until the attached effect is removed.
///
/// # `create_scale_anim(duration: Float (Optional)) -> ScriptScaleAnimation`
/// Creates a scale animation that will change the size of the entity by a
/// factor, for the specified duration.  If `duration` is not specified, the
/// animation lasts forever or until the attached effect is removed.
///
/// # `create_subpos_anim(duration: Float (Optional)) -> ScriptSubposAnimation`
/// Creates an entity subpos animation, that can be used to temporarily move
/// the location of the entity with pixel accuracy on the screen, for the specified
/// `duration` in seconds.  The animation is set up with further calls before
/// calling `activate()`.
///
/// # `create_color_anim(duration: Float (Optional)) -> ScriptColorAnimation`
/// Creates an entity color animation, which changes the primary and secondary
/// colors of the parent entity.  If `duration` is specified, lasts for that many seconds.
/// Otherwise, will last forever, or more typically until the attached effect is removed.
///
/// # `create_particle_generator(image: String, duration: Float (Optional)) ->
/// ScriptParticleGenerator`
/// Creates a Particle Generator animation.  Despite the name, can also be used for more
/// traditional frame based animations by using a single particle (see `create_anim`.
/// If `duration` is specified, lasts for that number of seconds.  Otherwise, will last
/// forever, or more typically until the attached effect is removed.  The specified image
/// must be the ID of a defined image.
///
/// # `create_anim(image: String, duration: Float (Optional)) -> ScriptParticleGenerator`
/// Creates a particle generator animation set up for a single particle frame based
/// animation.  The `image` should normally be the ID of a timer image with specified frames.
/// The `duration` is in seconds, or not specified to make the animation repeat until
/// the parent effect is removed (if there is one).  The anim must have `activate()` called
/// once setup is complete.
///
/// # `create_targeter(ability: ScriptAbility) -> TargeterData`
/// Creates a new targeter for the specified ability.  The ability's script will be used for
/// all functions.  This targeter can then be configured
/// before calling `activate()` to put it into effect.  Upon the user or ai script selecting
/// a target, `on_target_select` is called.
///
/// # `create_targeter_for_item(item: ScriptItem) -> TargeterData`
/// Creates a new targeter for the specified item.  The item's script will be used for all
/// functions.  The targeter can then be configured before calling `activate()`.  See
/// `create_targeter` above.
///
/// # `move_towards_entity(target: ScriptEntity, distance: Float (Optional)) -> Bool`
/// Causes this entity to attempt to begin moving towards the specified `target`.  If this
/// entity cannot move at all towards the desired target, returns false, otherwise, returns
/// true and creates a move animation that will proceed to be run asynchronously.
/// Optionally, a `distance` can be specified which is the distance this entity should be
/// within the target to complete the move.  If no distance is specified, the entity
/// attempts to move within attack range.
///
/// # `move_towards_point(x: Float, y: Float, distance: Float (Optional)) -> Bool`
/// Causes this entity to attempt to begin moving towards the specified point at
/// `x` and `y`.  If `distance` is specified, attempts to move within that distance
/// of the point.  Otherwise, attempts to move so the parent entity's coordinates
/// are equal to the nearest integers to `x` and `y`.  If the entity cannot move at
/// all or a path cannot be found, this returns false.  Otherwise, returns true and
/// an asynchronous move animation is initiated.
///
/// # `dist_to_entity(target: ScriptEntity) -> Float`
/// Computes the current euclidean distance to the specified `target`, in tiles.
///
/// # `dist_to_point(point: Table) -> Float`
/// Computes the euclidean distance to the specified `point`, in tiles.  Point is
/// a table of the form `{x: x_coord, y: y_coord}`
///
/// # `has_ap_to_attack() -> Bool`
/// Returns true if this entity has enough AP to issue a single attack, false otherwise.
///
/// # `can_reach(target: ScriptEntity) -> Bool`
/// Returns true if this entity can reach the `target` with a melee attack, false
/// otherwise.
///
/// # `has_visibility(target: ScriptEntity) -> Bool`
/// Returns true if this entity can see the `target`, false otherwise.
///
/// # `can_move() -> Bool`
/// Returns true if this entity can move at all (even 1 square), false otherwise.
///
/// # `teleport_to(dest: Table)`
/// Instantly moves this entity to the `dest`, which is a table of the form
/// `{ x: x_coord, y: y_coord }`.  Will not move the entity if the dest
/// position is invalid (outside area bounds, impassable).
///
/// # `weapon_attack(target: ScriptEntity) -> ScriptHitKind`
/// Immediately rolls a random attack against the specified `target`, using this
/// entities stats vs the defender. Returns the hit type, one of crit, hit,
/// graze, or miss.
///
/// # 'anim_weapon_attack(target: ScriptEntity, callback: CallbackData (Optional),
/// use_ap: Bool (Optional))`
/// Attempts to perform a standard weapon attack against the `target`.  The attack
/// is animated, so this method immediately returns but the attack happens
/// asynchronously.  Upon completion of the attack, the `callback` (if specified)
/// is run.  If `use_ap` is specified to false, no ap is deducted from the parent
/// for the attack.  By default, the standard amount of ap is deducted.
///
/// # `special_attack(target: ScriptEntity, attack_kind: String, accuracy_kind: String,
/// min_damage: Float, max_damage: Float, ap_damage: Float, damage_kind: String)`
/// Immediately rolls a random non-standard attack against the `target`, using the specified
/// parameters.  See `anim_special_attack`.
///
/// # `anim_special_attack(target: ScriptEntity, attack_kind: String, accuracy_kind: String,
/// min_damage: Float, max_damage: Float, ap_damage: Float, damage_kind: String,
/// callback: CallbackData (Optional))`
/// Animates a non standard attack against the `target` with the specified parameters.
/// AttackKind is one of `Melee`, `Ranged`, or `Spell`, and determines which of the attackers
/// attack types to use.  AccuracyKind is one of `Fortitude`, `Reflex`, `Will`, or `Dummy`
/// and determines which of the defenders defense stats to use.
/// The amount of damage is rolled randomly, between the `min_damage` and `max_damage`, with
/// the specified (`ap_damage`) amount of armor piercing.  This damage is then compared
/// against the defender's armor as normal.
/// If specified, the callback is called after the animation completes.  No ap is deducted
/// for this attack.
///
/// # `remove()`
/// Sets this entity to be removed (as if dead) on the next frame update.  This method
/// is called asynchronously, so the entity will not yet be removed immediately after
/// this method.
///
/// # `take_damage(attacker: ScriptEntity, min_damage: Float, max_damage: Float,
/// damage_kind: String, ap: Int (Optional))`
/// Causes this entity to take the specified amount of damage.  Hit points are removed,
/// based on this entity's armor.  The damage is rolled randomly between `min_damage` and
/// `max_damage`, with the specified (`ap`) amount of armor piercing.
///
/// # `heal_damage(amount: Float)`
/// Adds the specified number of hit points to this entity.  The entity's maximum hit
/// points cannot be exceeded in this way.
///
/// # `get_overflow_ap() -> Int`
/// Returns the current amount of overflow ap for this entity.  This is AP that will become
/// available as bonus AP (up to the maximum per round AP) on this entity's next turn.
///
/// # `change_overflow_ap(ap: Int)`
/// Modifies the amount of available overflow ap for this entity.  See `get_overflow_ap`.
///
/// # `set_subpos(x: Float, y: Float)`
/// Sets the pixel precise position of this entity to the specified value.  An entity should
/// generally not be left with non-zero values for either `x` or `y`.
///
/// # `remove_ap(amount: Int)`
/// Removes the specified `amount` of AP from this entity.  Keep in mind the `display_ap`
/// factor that this amount is divided by for display purposes.
///
/// # `base_class() -> String`
/// Returns the ID of the base class of this entity, or the class that this entity took at
/// level 1.
///
/// # `id() -> String`
/// Returns the ID of this entity.  This should be unique, but it is currently possible to have
/// more than one entity with the same ID (the game does provide a warning in this case).
///
/// # `name() -> String`
/// Returns the name of this entity.
///
/// # `has_ability(ability_id: String) -> Bool`
/// Returns true if this entity possesses the ability with the specified `ability_id`, false
/// otherwise.
///
/// # `get_ability(ability_id: String) -> ScriptAbility`
/// Returns a `ScriptAbility` representing the ability with the specified `ability_id`.  Throws
/// an error if this entity does not possess the ability.
///
/// # `ability_level(ability: ScriptAbility) -> Int`
/// Returns the level of the specified `ability` for this entity.  This is zero if the entity
/// does not possess the ability, one if it possesses just the base ability, and larger numbers
/// depending on the number of upgrades possessed.
///
/// # `has_active_mode() -> Bool`
/// Returns true if this entity has at least one currently active mode ability, false
/// otherwise.
///
/// # `stats() -> Table`
/// Creates and returns a stats table for this entity.  This includes all stats shown on the
/// character sheet.
///
/// # `inventory() -> ScriptInventory`
/// Returns a `ScriptInventory` object representing this entity's inventory.
///
/// # `race() -> String`
/// Returns the ID of the race of this entity
///
/// # `image_layer_offset(layer: String) -> Table`
/// Gets the image layer offset, in tiles for the given image layer
/// for this entity.  The table has members `x` and `y` with the offset value.
/// The layer must be a valid ImageLayer, one of HeldMain, HeldOff, Ears, Hair,
/// Beard, Head, Hands, Foreground, Torso, Legs, Feet, Background, Cloak, Shadow
///
/// # `size_str() -> String`
/// Returns the ID of the size of this entity, i.e. 2by2 or 3by3.
///
/// # `width() -> Int`
/// Returns the width of this entity in tiles
///
/// # `height() -> Int`
/// Returns the height of this entity in tiles
///
/// # `x() -> Int`
/// Returns the x coordinate of this entity's position in tiles
///
/// # `y() -> Int`
/// Returns the y coordinate of this entity's position in tiles
///
/// # `center_x() -> Float`
/// Returns the position of this entity's center (x + width / 2) as a float.
///
/// # `center_y() -> Float`
/// Returns the position of this entity's center (y + height / 2) as a float.
///
/// # `is_threatened() -> Bool`
/// Returns whether or not this entity is currently threatened by a hostile
/// with a melee weapon
#[derive(Clone, Debug)]
pub struct ScriptEntity {
    pub index: Option<usize>,
}

impl ScriptEntity {
    pub fn invalid() -> ScriptEntity {
        ScriptEntity { index: None }
    }

    pub fn new(index: usize) -> ScriptEntity {
        ScriptEntity { index: Some(index) }
    }

    pub fn from(entity: &Rc<RefCell<EntityState>>) -> ScriptEntity {
        ScriptEntity { index: Some(entity.borrow().index()) }
    }

    pub fn check_not_equal(&self, other: &ScriptEntity) -> Result<()> {
        if self.index == other.index {
            warn!("Parent and target must not refer to the same entity for this method");
            Err(rlua::Error::FromLuaConversionError {
                from: "ScriptEntity",
                to: "ScriptEntity",
                message: Some("Parent and target must not match".to_string())
            })
        } else {
            Ok(())
        }
    }

    pub fn try_unwrap_index(&self) -> Result<usize> {
        match self.index {
            None => Err(rlua::Error::FromLuaConversionError {
                from: "ScriptEntity",
                to: "EntityState",
                message: Some("ScriptEntity does not have a valid index".to_string())
            }),
            Some(index) => Ok(index),
        }
    }

    pub fn try_unwrap(&self) -> Result<Rc<RefCell<EntityState>>> {
        match self.index {
            None => Err(rlua::Error::FromLuaConversionError {
                from: "ScriptEntity",
                to: "EntityState",
                message: Some("ScriptEntity does not have a valid index".to_string())
            }),
            Some(index) => {
                let mgr = GameState::turn_manager();
                let mgr = mgr.borrow();
                match mgr.entity_checked(index) {
                    None => Err(rlua::Error::FromLuaConversionError {
                        from: "ScriptEntity",
                        to: "EntityState",
                        message: Some("ScriptEntity refers to an entity that no longer exists.".to_string())
                    }),
                    Some(entity) => Ok(entity),
                }
            }
        }
    }
}

impl UserData for ScriptEntity {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("state_end", |_, _, ()| {
            Ok(ai::State::End)
        });

        methods.add_method("state_wait", |_, _, time: u32| {
            Ok(ai::State::Wait(time))
        });

        methods.add_method("vis_dist", |_, entity, ()| {
            let parent = entity.try_unwrap()?;
            let area_id = &parent.borrow().location.area_id;
            let area = GameState::get_area_state(area_id).unwrap();
            let dist = area.borrow().area.vis_dist as f32;
            Ok(dist)
        });

        methods.add_method("add_xp", |_, entity, amount: u32| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().actor.add_xp(amount);
            Ok(())
        });

        methods.add_method("add_levels", |_, entity, (class, levels): (String, u32)| {
            let entity = entity.try_unwrap()?;

            let class = match Module::class(&class) {
                None => {
                    warn!("Invalid class '{}' in script", class);
                    return Ok(());
                }, Some(class) => class,
            };

            let actor = {
                let old_actor = &entity.borrow().actor.actor;
                let xp = entity.borrow().actor.xp();
                Actor::from(old_actor, Some((class, levels)), xp, Vec::new(),
                    InventoryBuilder::default())
            };

            entity.borrow_mut().actor.replace_actor(actor);

            Ok(())
        });

        methods.add_method("add_to_party", |_, entity, show_portrait: Option<bool>| {
            let entity = entity.try_unwrap()?;
            GameState::add_party_member(entity, show_portrait.unwrap_or(true));
            Ok(())
        });

        methods.add_method("remove_from_party", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            GameState::remove_party_member(entity);
            Ok(())
        });

        methods.add_method("set_faction", |_, entity, faction: String| {
            let entity = entity.try_unwrap()?;

            match Faction::from_str(&faction) {
                None => warn!("Invalid faction '{}' in script", faction),
                Some(faction) => entity.borrow_mut().actor.set_faction(faction),
            }

            let mgr = GameState::turn_manager();
            let area_state = GameState::area_state();

            mgr.borrow_mut().check_ai_activation(&entity, &mut area_state.borrow_mut());

            Ok(())
        });

        methods.add_method("get_num_flag", |_, entity, flag: String| {
            let entity = entity.try_unwrap()?;
            let val = entity.borrow().get_num_flag(&flag);
            Ok(val)
        });

        methods.add_method("add_num_flag", |_, entity, (flag, val): (String, f32)| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().add_num_flag(&flag, val);
            Ok(())
        });

        methods.add_method("set_flag", |_, entity, (flag, val): (String, Option<String>)| {
            let entity = entity.try_unwrap()?;
            let val = match &val {
                None => "true",
                Some(val) => val,
            };

            entity.borrow_mut().set_custom_flag(&flag, val);
            Ok(())
        });

        methods.add_method("clear_flag", |_, entity, flag: String| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().clear_custom_flag(&flag);
            Ok(())
        });

        methods.add_method("has_flag", |_, entity, flag: String| {
            let entity = entity.try_unwrap()?;
            let result = entity.borrow().has_custom_flag(&flag);
            Ok(result)
        });

        methods.add_method("get_flag", |_, entity, flag: String| {
            let entity = entity.try_unwrap()?;
            let result = entity.borrow().get_custom_flag(&flag);
            Ok(result)
        });

        methods.add_method("is_dead", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let result = entity.borrow().actor.is_dead();
            Ok(result)
        });

        methods.add_method("is_valid", |_, entity, ()| {
            let mgr = GameState::turn_manager();
            match entity.index {
                None => Ok(false),
                Some(index) => Ok(mgr.borrow().has_entity(index)),
            }
        });

        methods.add_method("is_party_member", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let is_member = entity.borrow().is_party_member();
            Ok(is_member)
        });

        methods.add_method("use_ability", |_, entity, ability: ScriptAbility| {
            let parent = entity.try_unwrap()?;
            if !parent.borrow().actor.can_toggle(&ability.id) { return Ok(false); }
            GameState::execute_ability_on_activate(&parent, &ability.to_ability());
            Ok(true)
        });

        methods.add_method("use_item", |_, entity, item: ScriptUsableItem| {
            let slot = item.slot;
            let parent = entity.try_unwrap()?;
            if !parent.borrow().actor.can_use_quick(slot) { return Ok(false); }
            GameState::execute_item_on_activate(&parent, ScriptItemKind::Quick(slot));
            Ok(true)
        });

        methods.add_method("swap_weapons", |_, entity, ()| {
            let parent = entity.try_unwrap()?;
            if !parent.borrow().actor.can_swap_weapons() { return Ok(false); }

            parent.borrow_mut().actor.swap_weapon_set();
            Ok(true)
        });

        methods.add_method("abilities", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            Ok(ScriptAbilitySet::from(&entity))
        });

        methods.add_method("targets", &targets);

        methods.add_method("remove_effects_with_tag", |_, entity, tag: String| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();

            let mgr = GameState::turn_manager();
            let mut mgr = mgr.borrow_mut();

            for effect_index in entity.actor.effects_iter() {
                let effect = mgr.effect_mut(*effect_index);
                if effect.tag == tag {
                    effect.mark_for_removal();
                }
            }

            Ok(())
        });

        methods.add_method("create_surface", |_, _, (name, points, duration):
            (String, Vec<HashMap<String, i32>>, Option<u32>)| {
            let duration = match duration {
                None => ExtInt::Infinity,
                Some(dur) => ExtInt::Int(dur),
            };
            let points: Vec<(i32, i32)> = points.into_iter().map(|p| {
                let x = p.get("x").unwrap();
                let y = p.get("y").unwrap();
                (*x, *y)
            }).collect();
            Ok(ScriptEffect::new_surface(points, &name, duration))
        });

        methods.add_method("create_effect", |_, entity, args: (String, Option<u32>)| {
            let duration = match args.1 {
                None => ExtInt::Infinity,
                Some(dur) => ExtInt::Int(dur),
            };
            let ability = args.0;
            let index = entity.try_unwrap_index()?;
            Ok(ScriptEffect::new_entity(index, &ability, duration))
        });

        methods.add_method("create_image_layer_anim", |_, entity, duration_secs: Option<f32>| {
            let index = entity.try_unwrap_index()?;
            let duration = match duration_secs {
                None => ExtInt::Infinity,
                Some(amount) => ExtInt::Int((amount * 1000.0) as u32),
            };

            Ok(ScriptImageLayerAnimation::new(index, duration))
        });

        methods.add_method("create_scale_anim", |_, entity, duration_secs: Option<f32>| {
            let index = entity.try_unwrap_index()?;
            let duration = match duration_secs {
                None => ExtInt::Infinity,
                Some(amount) => ExtInt::Int((amount * 1000.0) as u32),
            };

            Ok(ScriptScaleAnimation::new(index, duration))
        });

        methods.add_method("create_subpos_anim", |_, entity, duration_secs: f32| {
            let index = entity.try_unwrap_index()?;
            let duration = ExtInt::Int((duration_secs * 1000.0) as u32);
            Ok(ScriptSubposAnimation::new(index, duration))
        });

        methods.add_method("create_color_anim", |_, entity, duration_secs: Option<f32>| {
            let index = entity.try_unwrap_index()?;
            let duration = match duration_secs {
                None => ExtInt::Infinity,
                Some(amount) => ExtInt::Int((amount * 1000.0) as u32),
            };
            Ok(ScriptColorAnimation::new(index, duration))
        });

        methods.add_method("create_particle_generator", |_, entity, args: (String, Option<f32>)| {
            let sprite = args.0;
            let index = entity.try_unwrap_index()?;
            let duration = match args.1 {
                None => ExtInt::Infinity,
                Some(amount) => ExtInt::Int((amount * 1000.0) as u32),
            };
            Ok(ScriptParticleGenerator::new(index, sprite, duration))
        });

        methods.add_method("wait_anim", |_, entity, duration: f32| {
            let index = entity.try_unwrap_index()?;
            let image = ResourceSet::get_empty_image();
            let duration = ExtInt::Int((duration * 1000.0) as u32);
            Ok(ScriptParticleGenerator::new_anim(index, image.id(), duration))
        });

        methods.add_method("create_anim", |_, entity, (image, duration): (String, Option<f32>)| {
            let duration = match duration {
                None => ExtInt::Infinity,
                Some(amount) => ExtInt::Int((amount * 1000.0) as u32),
            };
            let index = entity.try_unwrap_index()?;
            Ok(ScriptParticleGenerator::new_anim(index, image, duration))
        });

        methods.add_method("create_targeter", |_, entity, ability: ScriptAbility| {
            let index = entity.try_unwrap_index()?;
            Ok(TargeterData::new_ability(index, &ability.id))
        });

        methods.add_method("create_targeter_for_item", |_, entity, item: ScriptItem| {
            let index = entity.try_unwrap_index()?;
            Ok(TargeterData::new_item(index, item.kind))
        });

        methods.add_method("move_towards_entity", |_, entity, (dest, dist):
                           (ScriptEntity, Option<f32>)| {
            let parent = entity.try_unwrap()?;
            let target = dest.try_unwrap()?;

            if let Some(dist) = dist {
                let (x, y) = {
                    let target = target.borrow();
                    (target.location.x as f32 + (target.size.width / 2) as f32,
                     target.location.y as f32 + (target.size.height / 2) as f32)
                };
                Ok(GameState::move_towards_point(&parent, Vec::new(), x, y, dist, None))
            } else {
                Ok(GameState::move_towards(&parent, &target))
            }
        });

        methods.add_method("move_towards_point", |_, entity, (x, y, dist):
                           (f32, f32, Option<f32>)| {

            let parent = entity.try_unwrap()?;

            let dist = dist.unwrap_or(MOVE_TO_THRESHOLD);
            Ok(GameState::move_towards_point(&parent, Vec::new(), x, y, dist, None))
        });

        methods.add_method("has_ap_to_attack", |_, entity, ()| {
            let parent = entity.try_unwrap()?;
            let result = parent.borrow().actor.has_ap_to_attack();
            Ok(result)
        });

        methods.add_method("can_reach", |_, entity, target: ScriptEntity| {
            let parent = entity.try_unwrap()?;
            let target = target.try_unwrap()?;
            let result = parent.borrow().can_reach(&target);
            Ok(result)
        });

        methods.add_method("has_visibility", |_, entity, target: ScriptEntity| {
            let parent = entity.try_unwrap()?;
            let target = target.try_unwrap()?;
            let area_state = GameState::area_state();
            let area_state = area_state.borrow();
            let result = area_state.has_visibility(&parent.borrow(), &target.borrow());
            Ok(result)
        });

        methods.add_method("can_move", |_, entity, ()| {
            let parent = entity.try_unwrap()?;
            let result = parent.borrow().can_move();
            Ok(result)
        });

        methods.add_method("teleport_to", |_, entity, dest: HashMap<String, i32>| {
            let (x, y) = unwrap_point(dest)?;
            let entity = entity.try_unwrap()?;
            let entity_index = entity.borrow().index();
            let mgr = GameState::turn_manager();

            let area_state = GameState::area_state();
            if !entity.borrow().location.is_in(&area_state.borrow()) {
                let old_area_state = GameState::get_area_state(
                    &entity.borrow().location.area_id).unwrap();

                let surfaces = old_area_state.borrow_mut().remove_entity(&entity, &mgr.borrow());
                for surface in surfaces {
                    mgr.borrow_mut().remove_from_surface(entity_index, surface);
                }

                let new_loc = Location::new(x, y, &area_state.borrow().area);
                match area_state.borrow_mut().transition_entity_to(&entity, entity_index, new_loc) {
                    Err(e) => {
                        warn!("Unable to move entity using script function");
                        warn!("{}", e);
                    }, Ok(_) => (),
                }
            } else {
                let mut area_state = area_state.borrow_mut();
                area_state.move_entity(&entity, x, y, 0);
            }

            Ok(())
        });

        methods.add_method("weapon_attack", |_, entity, target: ScriptEntity| {
            let target = target.try_unwrap()?;
            let parent = entity.try_unwrap()?;
            let (hit_kind, damage, text, color) = ActorState::weapon_attack(&parent, &target);

            let area_state = GameState::area_state();
            area_state.borrow_mut().add_feedback_text(text, &target, color);

            let hit_kind = ScriptHitKind::new(hit_kind, damage);
            Ok(hit_kind)
        });

        methods.add_method("anim_weapon_attack", |_, entity, (target, callback, use_ap):
                           (ScriptEntity, Option<CallbackData>, Option<bool>)| {
            entity.check_not_equal(&target)?;
            let parent = entity.try_unwrap()?;
            let target = target.try_unwrap()?;

            let cb: Option<Box<ScriptCallback>> = match callback {
                None => None,
                Some(cb) => Some(Box::new(cb)),
            };

            let use_ap = use_ap.unwrap_or(false);

            EntityState::attack(&parent, &target, cb, use_ap);
            Ok(())
        });

        methods.add_method("anim_special_attack", |_, entity,
            (target, attack_kind, accuracy_kind, min_damage, max_damage, ap, damage_kind, cb):
            (ScriptEntity, String, String, f32, f32, f32, String, Option<CallbackData>)| {

            let min_damage = min_damage as u32;
            let max_damage = max_damage as u32;
            let ap = ap as u32;

            entity.check_not_equal(&target)?;
            let parent = entity.try_unwrap()?;
            let target = target.try_unwrap()?;
            let damage_kind = DamageKind::from_str(&damage_kind);
            let attack_kind = AttackKind::from_str(&attack_kind, &accuracy_kind);
            let mut cbs: Vec<Box<ScriptCallback>> = Vec::new();
            if let Some(cb) = cb {
                cbs.push(Box::new(cb));
            }
            let time = Config::animation_base_time_millis() * 5;
            let anim = animation::melee_attack_animation::new(&Rc::clone(&parent), &target,
                                                              time, cbs, Box::new(move |att, def| {
                let mut attack = Attack::special(&parent.borrow().actor.stats,
                    min_damage, max_damage, ap, damage_kind, attack_kind.clone());

                ActorState::attack(att, def, &mut attack)
            }));

            GameState::add_animation(anim);
            Ok(())
        });

        methods.add_method("special_attack", |_, entity,
            (target, attack_kind, accuracy_kind, min_damage, max_damage, ap, damage_kind):
            (ScriptEntity, String, String, Option<f32>, Option<f32>, Option<f32>, Option<String>)| {
            let target = target.try_unwrap()?;
            let parent = entity.try_unwrap()?;

            let damage_kind = match damage_kind {
                None => DamageKind::Raw,
                Some(ref kind) => DamageKind::from_str(kind),
            };
            let attack_kind = AttackKind::from_str(&attack_kind, &accuracy_kind);

            let min_damage = min_damage.unwrap_or(0.0) as u32;
            let max_damage = max_damage.unwrap_or(0.0) as u32;
            let ap = ap.unwrap_or(0.0) as u32;

            let mut attack = Attack::special(&parent.borrow().actor.stats,
                min_damage, max_damage, ap, damage_kind, attack_kind);

            let (hit_kind, damage, text, color) =
                ActorState::attack(&parent, &target, &mut attack);

            let area_state = GameState::area_state();
            area_state.borrow_mut().add_feedback_text(text, &target, color);

            let hit_kind = ScriptHitKind::new(hit_kind, damage);
            Ok(hit_kind)
        });

        methods.add_method("remove", |_, entity, ()| {
            let parent = entity.try_unwrap()?;
            parent.borrow_mut().marked_for_removal = true;
            Ok(())
        });

        methods.add_method("take_damage", |_, entity, (attacker, min_damage, max_damage, damage_kind, ap):
                           (ScriptEntity, f32, f32, String, Option<u32>)| {
            let parent = entity.try_unwrap()?;
            let attacker = attacker.try_unwrap()?;
            let damage_kind = DamageKind::from_str(&damage_kind);

            let min_damage = min_damage as u32;
            let max_damage = max_damage as u32;
            let damage = {
                let parent = &parent.borrow().actor.stats;
                let attack = Attack::special(parent, min_damage, max_damage, ap.unwrap_or(0),
                    damage_kind, AttackKind::Dummy);
                attack.roll_damage(&parent.armor, &parent.resistance, 1.0)
            };

            let (text, color) = if !damage.is_empty() {
                let mut total = 0;
                for (_, amount) in damage.iter() {
                    total += amount;
                }

                EntityState::remove_hp(&parent, &attacker, HitKind::Hit, damage);
                (format!("{}", total), ColorKind::Hit)
            } else {
                ("0".to_string(), ColorKind::Miss)
            };

            let area_state = GameState::area_state();
            area_state.borrow_mut().add_feedback_text(text, &parent, color);
            Ok(())
        });

        methods.add_method("heal_damage", |_, entity, amount: f32| {
            let amount = amount as u32;
            let parent = entity.try_unwrap()?;
            parent.borrow_mut().actor.add_hp(amount);
            let area_state = GameState::area_state();
            area_state.borrow_mut().add_feedback_text(format!("{}", amount), &parent,
                ColorKind::Heal);

            Ok(())
        });

        methods.add_method("get_overflow_ap", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let ap = entity.borrow().actor.overflow_ap();
            Ok(ap)
        });

        methods.add_method("change_overflow_ap", |_, entity, ap| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().actor.change_overflow_ap(ap);
            Ok(())
        });

        methods.add_method("set_subpos", |_, entity, (x, y): (f32, f32)| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().sub_pos = (x, y);
            Ok(())
        });

        methods.add_method("remove_ap", |_, entity, ap| {
            let entity = entity.try_unwrap()?;
            entity.borrow_mut().actor.remove_ap(ap);
            Ok(())
        });

        methods.add_method("base_class", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.actor.actor.base_class().id.clone())
        });

        methods.add_method("id", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.unique_id().to_string())
        });

        methods.add_method("name", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.actor.actor.name.to_string())
        });

        methods.add_method("has_ability", |_, entity, id: String| {
            let entity = entity.try_unwrap()?;
            let has = entity.borrow().actor.actor.has_ability_with_id(&id);
            Ok(has)
        });

        methods.add_method("get_ability", |_, entity, id: String| {
            let ability = match Module::ability(&id) {
                None => return Err(rlua::Error::FromLuaConversionError {
                    from: "String",
                    to: "ScriptAbility",
                    message: Some(format!("Ability '{}' does not exist", id))
                }),
                Some(ability) => ability,
            };
            let entity = entity.try_unwrap()?;
            if !entity.borrow().actor.actor.has_ability(&ability) {
                return Ok(None);
            }

            Ok(Some(ScriptAbility::from(&ability)))
        });

        methods.add_method("ability_level", |_, entity, ability: ScriptAbility| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();

            match entity.actor.actor.ability_level(&ability.id) {
                None => Ok(0),
                Some(level) => Ok(level + 1),
            }
        });

        methods.add_method("has_active_mode", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            for (_, ref state) in entity.actor.ability_states.iter() {
                if state.is_active_mode() { return Ok(true); }
            }
            Ok(false)
        });

        methods.add_method("stats", &create_stats_table);

        methods.add_method("race", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let race_id = entity.borrow().actor.actor.race.id.to_string();
            Ok(race_id)
        });

        methods.add_method("image_layer_offset", |_, entity, layer: String| {
            let layer = match ImageLayer::from_str(&layer) {
                Err(e) => return Err(rlua::Error::FromLuaConversionError {
                    from: "String",
                    to: "ImageLayer",
                    message: Some(e.to_string())
                }),
                Ok(layer) => layer,
            };

            let entity = entity.try_unwrap()?;
            let offset = entity.borrow().actor.actor.race
                .get_image_layer_offset(layer).unwrap_or(&(0.0, 0.0)).clone();
            let mut table: HashMap<&str, f32> = HashMap::new();
            table.insert("x", offset.0);
            table.insert("y", offset.1);
            Ok(table)
        });

        methods.add_method("inventory", |_, entity, ()| {
            Ok(ScriptInventory::new(entity.clone()))
        });

        methods.add_method("size_str", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.size().to_string())
        });
        methods.add_method("width", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.size.width)
        });
        methods.add_method("height", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.size.height)
        });
        methods.add_method("x", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let x = entity.borrow().location.x;
            Ok(x)
        });
        methods.add_method("y", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let y = entity.borrow().location.y;
            Ok(y)
        });
        methods.add_method("center_x", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let x = entity.borrow().location.x as f32 + entity.borrow().size.width as f32 / 2.0;
            Ok(x)
        });

        methods.add_method("center_y", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let y = entity.borrow().location.y as f32 + entity.borrow().size.height as f32 / 2.0;
            Ok(y)
        });

        methods.add_method("dist_to_entity", |_, entity, target: ScriptEntity| {
            let entity = entity.try_unwrap()?;
            let target = target.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.dist_to_entity(&target))
        });

        methods.add_method("dist_to_point", |_, entity, point: HashMap<String, i32>| {
            let (x, y) = unwrap_point(point)?;
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.dist_to_point(Point::new(x, y)))
        });

        methods.add_method("is_threatened", |_, entity, ()| {
            let entity = entity.try_unwrap()?;
            let entity = entity.borrow();
            Ok(entity.actor.is_threatened())
        });
    }
}

pub fn unwrap_point(point: HashMap<String, i32>) -> Result<(i32, i32)> {
    let x = match point.get("x") {
        None => return Err(rlua::Error::FromLuaConversionError {
            from: "ScriptPoint",
            to: "Point",
            message: Some("Point must have x and y coordinates".to_string())
        }),
        Some(x) => *x,
    };

    let y = match point.get("y") {
        None => return Err(rlua::Error::FromLuaConversionError {
            from: "ScriptPoint",
            to: "Point",
            message: Some("Point must have x and y coordinates".to_string())
        }),
        Some(y) => *y,
    };

    Ok((x, y))
}

fn create_stats_table<'a>(lua: &'a Lua, parent: &ScriptEntity, _args: ()) -> Result<rlua::Table<'a>> {
    let rules = Module::rules();

    let parent = parent.try_unwrap()?;
    let parent = parent.borrow();
    let src = &parent.actor.stats;

    let stats = lua.create_table()?;
    stats.set("current_hp", parent.actor.hp())?;
    stats.set("current_ap", parent.actor.ap())?;
    stats.set("current_xp", parent.actor.xp())?;

    stats.set("strength", src.attributes.strength)?;
    stats.set("dexterity", src.attributes.dexterity)?;
    stats.set("endurance", src.attributes.endurance)?;
    stats.set("perception", src.attributes.perception)?;
    stats.set("intellect", src.attributes.intellect)?;
    stats.set("wisdom", src.attributes.wisdom)?;

    {
        use self::Attribute::*;
        stats.set("strength_bonus", src.attributes.bonus(Strength, rules.base_attribute))?;
        stats.set("dexterity_bonus", src.attributes.bonus(Dexterity, rules.base_attribute))?;
        stats.set("endurance_bonus", src.attributes.bonus(Endurance, rules.base_attribute))?;
        stats.set("perception_bonus", src.attributes.bonus(Perception, rules.base_attribute))?;
        stats.set("intellect_bonus", src.attributes.bonus(Intellect, rules.base_attribute))?;
        stats.set("wisdom_bonus", src.attributes.bonus(Wisdom, rules.base_attribute))?;
    }

    stats.set("base_armor", src.armor.base())?;
    let armor = lua.create_table()?;
    for kind in DamageKind::iter() {
        armor.set(kind.to_str(), src.armor.amount(*kind))?;
    }
    stats.set("armor", armor)?;

    let resistance = lua.create_table()?;
    for kind in DamageKind::iter() {
        resistance.set(kind.to_str(), src.resistance.amount(*kind))?;
    }
    stats.set("resistance", resistance)?;

    stats.set("level", parent.actor.actor.total_level)?;
    stats.set("caster_level", src.caster_level)?;
    stats.set("bonus_reach", src.bonus_reach)?;
    stats.set("bonus_range", src.bonus_range)?;
    stats.set("max_hp", src.max_hp)?;
    stats.set("initiative", src.initiative)?;
    stats.set("melee_accuracy", src.melee_accuracy)?;
    stats.set("ranged_accuracy", src.ranged_accuracy)?;
    stats.set("spell_accuracy", src.spell_accuracy)?;
    stats.set("defense", src.defense)?;
    stats.set("fortitude", src.fortitude)?;
    stats.set("reflex", src.reflex)?;
    stats.set("will", src.will)?;

    stats.set("attack_distance", src.attack_distance() + parent.size.diagonal / 2.0)?;
    stats.set("attack_is_melee", src.attack_is_melee())?;
    stats.set("attack_is_ranged", src.attack_is_ranged())?;

    stats.set("concealment", src.concealment)?;
    stats.set("concealment_ignore", src.concealment_ignore)?;
    stats.set("crit_threshold", src.crit_threshold)?;
    stats.set("graze_threshold", src.graze_threshold)?;
    stats.set("hit_threshold", src.hit_threshold)?;
    stats.set("graze_multiplier", src.graze_multiplier)?;
    stats.set("hit_multiplier", src.hit_multiplier)?;
    stats.set("crit_multiplier", src.crit_multiplier)?;
    stats.set("movement_rate", src.movement_rate)?;
    stats.set("attack_cost", src.attack_cost)?;

    if let Some(image) = src.get_ranged_projectile() {
        stats.set("ranged_projectile", image.id())?;
    }

    for (index, attack) in src.attacks.iter().enumerate() {
        stats.set(format!("damage_min_{}", index), attack.damage.min())?;
        stats.set(format!("damage_max_{}", index), attack.damage.max())?;
        stats.set(format!("armor_penetration_{}", index), attack.damage.ap())?;
    }

    Ok(stats)
}

/// Represents a set of ScriptEntities, which can be created from a variety of
/// sources.  This is passed to many script functions as a `targets` variable.
/// It includes a parent ScriptEntity, a list of target ScriptEntities,
/// optionally a selected point (for a targeter that has been activated), and
/// optionally a list of affected points (again for a targeter).
///
/// # `to_table() -> Table`
/// Creates a table of this set.  Iterating over the table will allow you
/// to access each entity in this set.
/// ## Examples
/// ```lua
///   table = targets:to_table()
///   for i = 1, #table do
///    game:log("target: " .. table[i]:name())
///   end
/// ```
///
/// # `random_affected_points(frac: Float) -> Table`
/// Returns a table of a randomly selected subset of the affected points in this
/// set.  The probability of any individual point ending up in the returned set
/// is set by `frac`.
///
/// # `surface() -> ScriptActiveSurface`
/// Returns the surface associated with this target set, if it is defined.  Otherwise
/// throws an error.
///
/// # `affected_points() -> Table`
/// Returns a table containing all the affected points in this set.
/// ## Examples
/// ```lua
///   points = targets:affected_points()
///   for i = 1, #points do
///     point = points[i]
///     game:log("point " .. point.x .. ", " .. point.y)
///   end
/// ```
///
/// # `selected_point() -> Table`
/// Returns a table representing the selected point for this set, if one is defined.
/// The table will have `x` and `y` elements defined.  If there is no selected point,
/// throws an error.
///
/// # `is_empty() -> Bool`
/// Returns whether or not there are any targets in this ScriptEntitySet.  Does not
/// take affected points or selected_point into consideration.
///
/// # `first() -> ScriptEntity`
/// Returns the first ScriptEntity as a target in this set, or throws an error if the
/// set is empty.
///
/// # `parent() -> ScriptEntity`
/// Returns the parent ScriptEntity of this set.  When this is passed to a function as
/// `targets`, usually, but not always, the `parent` argument is the same as this.
///
/// # `without_self() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet which contains all the data in this set, except
/// it does not include the parent entity as a target.
///
/// # `visible_within(dist: Float) -> ScriptEntitySet`
/// Creates a new ScriptEntitySet containing all the data in this set, except all
/// targets that are not visible or are outside the specified dist from the parent
/// are removed.
///
/// # `visible() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// that are visible to the parent are present.
///
/// # `hostile() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// that are hostile to the parent are present.
///
/// # `friendly() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// that are friendly to the parent are present.
///
/// # `reachable() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// which the parent can reach with a melee weapon are present.
///
/// # `attackable() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// which the parent can attack with their current weapon are present.  If the parent
/// does not have enough AP or otherwise cannot attack, the set will be empty.
///
/// # `threatening() -> ScriptEntitySet`
/// Creates a new ScriptEntitySet with all the data from this set, except only targets
/// which can hit the parent with a melee weapon currently or in the future without moving
/// are present.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScriptEntitySet {
    pub parent: usize,
    pub selected_point: Option<(i32, i32)>,
    pub affected_points: Vec<(i32, i32)>,
    pub indices: Vec<Option<usize>>,

    // surface is set when passing into script as argument, but should
    // never be saved as part of a callback
    #[serde(skip)]
    pub surface: Option<ScriptActiveSurface>,
}

impl ScriptEntitySet {
    pub fn update_entity_refs_on_load(&mut self,
                                      entities: &HashMap<usize, Rc<RefCell<EntityState>>>) ->
        ::std::result::Result<(), Error> {

        match entities.get(&self.parent) {
            None => {
                return invalid_data_error(
                    &format!("Invalid parent {} for ScriptEntitySet", self.parent));
            }, Some(ref entity) => self.parent = entity.borrow().index(),
        }

        let mut indices = Vec::new();
        for index in self.indices.drain(..) {
            match index {
                None => indices.push(None),
                Some(index) => {
                    match entities.get(&index) {
                        None => {
                            return invalid_data_error(
                                &format!("Invalid target {} for ScriptEntitySet", index));
                        }, Some(ref entity) => indices.push(Some(entity.borrow().index())),
                    }
                }
            }
        }
        self.indices = indices;

        Ok(())
    }

    pub fn append(&mut self, other: &ScriptEntitySet) {
        self.indices.append(&mut other.indices.clone());
        self.selected_point = other.selected_point.clone();
        self.affected_points.append(&mut other.affected_points.clone());
        self.surface = other.surface.clone();
    }

    pub fn with_parent(parent: usize) -> ScriptEntitySet {
        ScriptEntitySet {
            parent,
            indices: Vec::new(),
            selected_point: None,
            affected_points: Vec::new(),
            surface: None,
        }
    }

    pub fn from_pair(parent: &Rc<RefCell<EntityState>>,
                     target: &Rc<RefCell<EntityState>>) -> ScriptEntitySet {
        let parent = parent.borrow().index();
        let indices = vec![Some(target.borrow().index())];

        ScriptEntitySet {
            parent,
            selected_point: None,
            affected_points: Vec::new(),
            indices,
            surface: None,
        }
    }

    pub fn new(parent: &Rc<RefCell<EntityState>>,
               entities: &Vec<Option<Rc<RefCell<EntityState>>>>) -> ScriptEntitySet {
        let parent = parent.borrow().index();

        let indices = entities.iter().map(|e| {
            match e {
                &None => None,
                &Some(ref e) => Some(e.borrow().index()),
            }
        }).collect();
        ScriptEntitySet {
            parent,
            selected_point: None,
            affected_points: Vec::new(),
            indices,
            surface: None,
        }
    }
}

impl UserData for ScriptEntitySet {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("to_table", |_, set, ()| {
            let table: Vec<ScriptEntity> = set.indices.iter().map(|i| ScriptEntity { index: *i }).collect();

            Ok(table)
        });

        methods.add_method("random_affected_points", |_, set, frac: f32| {
            let table: Vec<HashMap<&str, i32>> = set.affected_points.iter().filter_map(|p| {
                let roll = rand::thread_rng().gen_range(0.0, 1.0);
                if roll > frac {
                    None
                } else {
                    let mut map = HashMap::new();
                    map.insert("x", p.0);
                    map.insert("y", p.1);
                    Some(map)
                }
            }).collect();
            Ok(table)
        });

        methods.add_method("surface", |_, set, ()| {
            match &set.surface {
                None => {
                    warn!("Attempted to get surface from target set with no surface defined");
                    Err(rlua::Error::FromLuaConversionError {
                        from: "ScriptEntitySet",
                        to: "Surface",
                        message: Some("EntitySet has no surface".to_string())
                    })
                }, Some(surf) => {
                    Ok(surf.clone())
                }
            }
        });

        methods.add_method("affected_points", |_, set, ()| {
            let table: Vec<HashMap<&str, i32>> = set.affected_points.iter().map(|p| {
                let mut map = HashMap::new();
                map.insert("x", p.0);
                map.insert("y", p.1);
                map
            }).collect();
            Ok(table)
        });

        methods.add_method("selected_point", |_, set, ()| {
            match set.selected_point {
                None => {
                    warn!("Attempted to get selected point from EntitySet where none is defined");
                    Err(rlua::Error::FromLuaConversionError {
                        from: "ScriptEntitySet",
                        to: "Point",
                        message: Some("EntitySet has no selected point".to_string())
                    })
                }, Some((x, y)) => {
                    let mut point = HashMap::new();
                    point.insert("x", x);
                    point.insert("y", y);
                    Ok(point)
                }
            }
        });
        methods.add_method("is_empty", |_, set, ()| Ok(set.indices.is_empty()));
        methods.add_method("first", |_, set, ()| {
            for index in set.indices.iter() {
                if let &Some(index) = index {
                    return Ok(ScriptEntity::new(index));
                }
            }

            warn!("Attempted to get first element of EntitySet that has no valid entities");
            Err(rlua::Error::FromLuaConversionError {
                from: "ScriptEntitySet",
                to: "ScriptEntity",
                message: Some("EntitySet is empty".to_string())
            })
        });

        methods.add_method("parent", |_, set, ()| {
            Ok(ScriptEntity::new(set.parent))
        });

        methods.add_method("without_self", &without_self);
        methods.add_method("visible_within", &visible_within);
        methods.add_method("visible", |lua, set, ()| visible_within(lua, set, std::f32::MAX));
        methods.add_method("hostile", |lua, set, ()| is_hostile(lua, set));
        methods.add_method("friendly", |lua, set, ()| is_friendly(lua, set));
        methods.add_method("reachable", &reachable);
        methods.add_method("attackable", &attackable);
        methods.add_method("threatening", &threatening);
    }
}

fn targets(_lua: &Lua, parent: &ScriptEntity, _args: ()) -> Result<ScriptEntitySet> {
    let parent = parent.try_unwrap()?;
    let area_id = parent.borrow().location.area_id.to_string();

    let mgr = GameState::turn_manager();
    let mut indices = Vec::new();
    for entity in mgr.borrow().entity_iter() {
        if parent.borrow().is_hostile(&entity) && entity.borrow().actor.stats.hidden {
            continue;
        }

        let entity = entity.borrow();
        if entity.actor.is_dead() { continue; }
        if !entity.location.is_in_area_id(&area_id) { continue; }

        indices.push(Some(entity.index()));
    }

    let parent_index = parent.borrow().index();
    Ok(ScriptEntitySet {
        parent: parent_index,
        indices,
        selected_point: None,
        affected_points: Vec::new(),
        surface: None,
    })
}

fn without_self(_lua: &Lua, set: &ScriptEntitySet, _: ()) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        !Rc::ptr_eq(parent, entity)
    })
}

fn visible_within(_lua: &Lua, set: &ScriptEntitySet, dist: f32) -> Result<ScriptEntitySet> {
    filter_entities(set, dist, &|parent, entity, dist| {
        if parent.borrow().dist_to_entity(entity) > dist { return false; }

        let area_state = GameState::area_state();
        let area_state = area_state.borrow();
        area_state.has_visibility(&parent.borrow(), &entity.borrow())
    })
}

fn attackable(_lua: &Lua, set: &ScriptEntitySet, _args: ()) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        let area_state = GameState::area_state();
        let area_state = area_state.borrow();
        parent.borrow().can_attack(entity, &area_state)
    })
}

fn threatening(_lua: &Lua, set: &ScriptEntitySet, _args: ()) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        let entity = entity.borrow();
        if !entity.actor.stats.attack_is_melee() { return false; }
        if entity.actor.stats.attack_disabled { return false; }

        entity.can_reach(parent)
    })
}

fn reachable(_lua: &Lua, set: &ScriptEntitySet, _args: ()) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        parent.borrow().can_reach(entity)
    })
}

fn is_hostile(_lua: &Lua, set: &ScriptEntitySet) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        parent.borrow().is_hostile(entity)
    })
}

fn is_friendly(_lua: &Lua, set: &ScriptEntitySet) -> Result<ScriptEntitySet> {
    filter_entities(set, (), &|parent, entity, _| {
        !parent.borrow().is_hostile(entity)
    })
}

fn filter_entities<T: Copy>(set: &ScriptEntitySet, t: T,
                  filter: &Fn(&Rc<RefCell<EntityState>>, &Rc<RefCell<EntityState>>, T) -> bool)
    -> Result<ScriptEntitySet> {

    let parent = ScriptEntity::new(set.parent);
    let parent = parent.try_unwrap()?;

    let mgr = GameState::turn_manager();
    let mgr = mgr.borrow();

    let mut indices = Vec::new();
    for index in set.indices.iter() {
        let entity = match index {
            &None => continue,
            &Some(index) => mgr.entity_checked(index),
        };

        let entity = match entity {
            None => continue,
            Some(entity) => entity,
        };

        if !(filter)(&parent, &entity, t) { continue; }

        indices.push(*index);
    }

    Ok(ScriptEntitySet {
        parent: set.parent,
        indices,
        selected_point: set.selected_point,
        affected_points: set.affected_points.clone(),
        surface: set.surface.clone(),
    })
}
