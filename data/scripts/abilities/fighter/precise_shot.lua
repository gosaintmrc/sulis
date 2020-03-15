function on_activate(parent, ability)
  local cur_mode = parent:get_active_mode()
  if cur_mode ~= nil then
    cur_mode:deactivate(parent)
  end

  local effect = parent:create_effect(ability:name())
  effect:deactivate_with(ability)
  effect:add_num_bonus("attack_cost", -1000)
  effect:add_damage(0, 0, 5)

  local stats = parent:stats()
  if parent:ability_level(ability) > 1 then
	effect:add_num_bonus("ranged_accuracy", 25 + stats.level)
	effect:add_num_bonus("crit_multiplier", 1.0)
  else
    effect:add_num_bonus("ranged_accuracy", 15 + stats.level / 2)
	effect:add_num_bonus("crit_multiplier", 0.50)
  end
  
  local cb = ability:create_callback(parent)
  cb:set_on_held_changed_fn("on_held_changed")
  effect:add_callback(cb)
  
  local gen = parent:create_anim("precise_arrow")
  gen:set_moves_with_parent()
  gen:set_position(gen:param(-0.5), gen:param(-2.5))
  gen:set_particle_size_dist(gen:fixed_dist(1.0), gen:fixed_dist(1.0))
  effect:add_anim(gen)
  effect:apply()

  game:play_sfx("sfx/sword_sheath_2")
  ability:activate(parent)
end

function on_deactivate(parent, ability)
  ability:deactivate(parent)
end

function on_held_changed(parent, ability)
  if not parent:stats().attack_is_ranged then
    game:say_line("Precise Shot Deactivated", parent)
    ability:deactivate(parent)
  end
end
