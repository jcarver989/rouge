use crate::{Div, IntoProp};
use accesskit::Node;

pub use accesskit::{
    Action, ActionData, ActionHandler, ActionRequest, ActivationHandler, Live, NodeId, Role,
    Toggled, TreeId, TreeUpdate,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Increment;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decrement;

#[derive(Default)]
pub(crate) struct Semantics {
    pub role: Option<Role>,
    pub node: Node,
    pub hidden: bool,
    pub activates: bool,
}

macro_rules! properties {
    ($($method:ident($variant:ident: $ty:ty) => $setter:ident;)*) => {
        pub(crate) enum AccessOp { $($variant($ty),)* Hidden(bool) }

        impl Div { $(
            pub fn $method<T>(self, value: impl IntoProp<$ty, T>) -> Self {
                self.access(value, AccessOp::$variant)
            }
        )* }

        impl Semantics {
            pub(crate) fn apply(&mut self, op: AccessOp) {
                match op {
                    $(AccessOp::$variant(value) => self.node.$setter(value),)*
                    AccessOp::Hidden(hidden) => self.hidden = hidden,
                }
            }
        }
    };
}

properties! {
    access_label(Label: String) => set_label;
    access_description(Description: String) => set_description;
    access_value(Value: String) => set_value;
    numeric_value(Numeric: f64) => set_numeric_value;
    expanded(Expanded: bool) => set_expanded;
    toggled(Toggled: Toggled) => set_toggled;
}
