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

use std::any::Any;
use std::rc::Rc;
use std::cell::RefCell;

use sulis_core::ui::{Callback, Widget, WidgetKind};
use sulis_widgets::{Label, Button, TextArea, ScrollPane};
use sulis_module::{Quest, Module};
use sulis_state::{quest_state::EntryState, GameState, ChangeListener};

pub const NAME: &str = "quest_window";

pub struct QuestWindow {
    active_quest: Option<Rc<Quest>>,
}

impl QuestWindow {
    pub fn new() -> Rc<RefCell<QuestWindow>> {
        Rc::new(RefCell::new(QuestWindow {
            active_quest: None,
        }))
    }
}

impl WidgetKind for QuestWindow {
    widget_kind!(NAME);

    fn on_add(&mut self, widget: &Rc<RefCell<Widget>>) -> Vec<Rc<RefCell<Widget>>> {
        GameState::add_quest_state_change_listener(ChangeListener::invalidate(NAME, widget));

        let quests = GameState::quest_state();

        let label = Widget::with_theme(Label::empty(), "title");

        let close = Widget::with_theme(Button::empty(), "close");
        close.borrow_mut().state.add_callback(Callback::remove_parent());

        let quest_list_pane = ScrollPane::new();
        let quest_list_widget = Widget::with_theme(quest_list_pane.clone(), "quest_list");

        for quest in Module::all_quests() {
            let selected = if let Some(ref active_quest) = self.active_quest {
                Rc::ptr_eq(active_quest, &quest)
            } else {
                false
            };

            let active = match quests.state(&quest.id) {
                EntryState::Hidden => continue,
                EntryState::Visible => selected,
                EntryState::Active => true,
                EntryState::Complete => selected,
            };

            let button = Widget::with_theme(Button::empty(), "quest_button");
            button.borrow_mut().state.set_active(active);

            let quest_ref = Rc::clone(&quest);
            button.borrow_mut().state.add_callback(Callback::new(Rc::new(move |widget, _| {
                let window = Widget::go_up_tree(&widget, 3);
                let quest_window = Widget::downcast_kind_mut::<QuestWindow>(&window);
                quest_window.active_quest = Some(Rc::clone(&quest_ref));
                window.borrow_mut().invalidate_children();
            })));

            let text_area = Widget::with_defaults(TextArea::empty());
            text_area.borrow_mut().state.add_text_arg("name", &quest.name);

            Widget::add_child_to(&button, text_area);

            quest_list_pane.borrow().add_to_content(button);
        }

        let quest_entries_pane = ScrollPane::new();
        let quest_entries_widget = Widget::with_theme(quest_entries_pane.clone(), "quest_entries");

        if let Some(ref quest) = self.active_quest {
            for (id, quest_entry) in quest.entries.iter() {
                let active = match quests.entry_state(&quest.id, id) {
                    EntryState::Hidden => continue,
                    EntryState::Visible => false,
                    EntryState::Active => true,
                    EntryState::Complete => false,
                };

                let entry = Widget::with_theme(TextArea::empty(), "quest_entry");

                {
                    let mut state = &mut entry.borrow_mut().state;
                    state.set_active(active);
                    state.add_text_arg("description", &quest_entry.description);
                }

                quest_entries_pane.borrow().add_to_content(entry);
            }
        }

        vec![label, close, quest_list_widget, quest_entries_widget]
    }
}