use grt::resource::Actor;
use state::Inventory;

use std::rc::Rc;
use std::cell::RefCell;

#[derive(Clone)]
pub struct ActorState {
    pub actor: Rc<Actor>,
    pub inventory: Rc<RefCell<Inventory>>,
}

impl PartialEq for ActorState {
    fn eq(&self, other: &ActorState) -> bool {
        Rc::ptr_eq(&self.actor, &other.actor)
    }
}

impl ActorState {
    pub fn new(actor: Rc<Actor>) -> ActorState {
        trace!("Creating new actor state for {}", actor.id);
        let inventory = Rc::new(RefCell::new(Inventory::new(&actor)));
        ActorState {
            actor,
            inventory,
        }
    }
}
