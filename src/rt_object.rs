/*
// location feature
struct RTObject {
    //parent: &RTObject,
    //path: Path,
    //debug_metadata: DebugMetadata,
}

impl RTObject {

}
*/

use std::any::Any;

pub trait RTObject {
    fn as_any(&self) -> &dyn Any;
}

pub struct Null;
impl RTObject for Null {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
