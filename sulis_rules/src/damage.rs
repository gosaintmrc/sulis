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

use std::fmt::{self, Display};
use rand::{self, Rng};

use Armor;

pub struct DamageList {
    damage: Vec<Damage>,
    min: u32,
    max: u32,
}

impl DamageList {
    pub fn new() -> DamageList {
        DamageList {
            damage: Vec::new(),
            min: 0,
            max: 0,
        }
    }

    pub fn create(&mut self, base_damage: Damage, bonus_damage: &Vec<Damage>) {
        if base_damage.kind.is_none() {
            warn!("Attempted to create damage list with no base damage kind");
            return;
        }

        let mut min = base_damage.min;
        let mut max = base_damage.max;
        self.damage.push(base_damage);

        let mut bonus_damage = bonus_damage.clone();
        bonus_damage.sort_by_key(|d| d.kind);

        let mut cur_damage = None;
        for damage in bonus_damage {
            min += damage.min;
            max += damage.max;

            if damage.kind.is_none() || damage.kind == self.damage[0].kind {
                self.damage[0].add(damage);
                continue;
            }

            match cur_damage {
                None => {
                    cur_damage = Some(damage);
                }, Some(mut cur_damage_unwrapped) => {
                    if cur_damage_unwrapped.kind == damage.kind {
                        cur_damage_unwrapped.add(damage);
                    } else {
                        assert!(cur_damage_unwrapped.kind.is_some());
                        self.damage.push(cur_damage_unwrapped);
                        cur_damage = Some(damage);
                    }
                }
            }
        }

        if let Some(cur_damage) = cur_damage {
            assert!(cur_damage.kind.is_some());
            self.damage.push(cur_damage);
        }

        self.min = min;
        self.max = max;

        debug!("Created damage list {} to {}, base kind {}", self.min,
               self.max, self.damage[0].kind.unwrap());
        for damage in self.damage.iter() {
            trace!("Component: {} to {}, kind {:?}", damage.min, damage.max, damage.kind);
        }
    }

    /// Computes the amount of damage that this damage list will apply to the given
    /// `armor`.  Each damage component of this list is rolled randomly, with the resulting
    /// damage then multiplied by the `multiplier`, rounded down.  The armor against
    /// the base damage kind of this damage is then subtracted from the damage.  The
    /// resulting vector may be an empty vector to indicate no damage, or a vector of
    /// one or more kinds each associated with a positive damage amount.  The damage
    /// amount will never be zero.
    pub fn roll(&self, armor: &Armor, multiplier: f32) -> Vec<(DamageKind, u32)> {
        if self.damage.is_empty() { return Vec::new(); }

        let armor_amount = armor.amount(self.damage[0].kind.unwrap());

        debug!("Computing damage amount from {} to {} vs {} armor", self.min,
               self.max, armor_amount);

        let mut output = Vec::new();
        let mut armor_left = armor_amount;
        for damage in self.damage.iter() {
            let mut damage_amount = (damage.roll() as f32 * multiplier) as u32;
            let kind = damage.kind.unwrap();

            if armor_left >= damage_amount {
                armor_left -= damage_amount;
            } else {
                damage_amount -= armor_left;
                armor_left = 0;
                output.push((kind, damage_amount));
            }
        }

        output
    }

    pub fn min(&self) -> u32 { self.min }

    pub fn max(&self) -> u32 { self.max }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub enum DamageKind {
    Slashing,
    Piercing,
    Crushing,
    Acid,
    Cold,
    Electrical,
    Fire,
    Sonic,
    Raw,
}

impl Display for DamageKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

#[derive(Deserialize, Debug, Copy, Clone)]
#[serde(deny_unknown_fields)]
pub struct Damage {
    pub min: u32,
    pub max: u32,
    pub kind: Option<DamageKind>,
}

impl Damage {
    pub fn add(&mut self, other: Damage) {
        self.min += other.min;
        self.max += other.max;
    }

    pub fn mult_f32(&mut self, val: f32) -> Damage {
        Damage {
            min: (self.min as f32 * val) as u32,
            max: (self.max as f32 * val) as u32,
            kind: self.kind,
        }
    }

    pub fn mult(&self, times: u32) -> Damage {
        Damage {
            min: self.min * times,
            max: self.max * times,
            kind: self.kind,
        }
    }

    pub fn average(&self) -> f32 {
        (self.min as f32 + self.max as f32) / 2.0
    }

    pub fn roll(&self) -> u32 {
        rand::thread_rng().gen_range(self.min, self.max + 1)
    }
}

impl Default for Damage {
    fn default() -> Damage {
        Damage {
            min: 0,
            max: 0,
            kind: None,
        }
    }
}
