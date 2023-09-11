#![allow(unused_variables, dead_code)]

use std::{rc::Rc, cell::RefCell};

use as_any::Downcast;

use crate::{json_serialization, container::{Container, self}, story_state::StoryState, object_enum::ObjectEnum};

const INK_VERSION_CURRENT: i32 = 21;
const INK_VERSION_MINIMUM_COMPATIBLE: i32 = 18;

pub struct Story{
    pub main_content_container: Rc<Container>,
    state: StoryState,
}

impl Story {
    pub fn new(json_string: &str) -> Result<Self, String> {
        let json: serde_json::Value = match serde_json::from_str(json_string) {
            Ok(value) => value,
            Err(_) => return Err("Story not in JSON format.".to_string()),
        };

        let version_opt = json.get("inkVersion");

        if version_opt.is_none() || !version_opt.unwrap().is_number() {
            return Err(
                "ink version number not found. Are you sure it's a valid .ink.json file?"
                    .to_string(),
            );
        }

        let version: i32 = version_opt.unwrap().as_i64().unwrap().try_into().unwrap();

        if version > INK_VERSION_CURRENT {
            return Err("Version of ink used to build story was newer than the current version of the engine".to_string());
        } else if version < INK_VERSION_MINIMUM_COMPATIBLE {
            return Err("Version of ink used to build story is too old to be loaded by this version of the engine".to_string());
        } else if version != INK_VERSION_CURRENT {
            log::debug!("WARNING: Version of ink used to build story doesn't match current version of engine. Non-critical, but recommend synchronising.");
        }

        let rootToken = match json.get("root") {
            Some(value) => value,
            None => {
                return Err(
                    "Root node for ink not found. Are you sure it's a valid .ink.json file?"
                        .to_string(),
                )
            }
        };

        //object listDefsObj;
        //if (rootObject.TryGetValue ("listDefs", out listDefsObj)) {
        //    _listDefinitions = Json.JTokenToListDefinitions (listDefsObj);
        //}


        let main_content_container = json_serialization::jtoken_to_runtime_object(rootToken)?;

        if main_content_container.as_any().downcast_ref::<Container>().is_none() {
            return Err("Root node for ink is not a container?".to_string());
        };

        let mut story = Story { main_content_container: main_content_container.downcast_ref::<Rc<Container>>().unwrap().clone(), state: StoryState::new()};

        story.reset_state();

        Ok(story)
    }

    fn reset_state(&mut self) {
        //TODO ifAsyncWeCant("ResetState");

        self.state = StoryState::new();

        // TODO state.getVariablesState().setVariableChangedEvent(this);

        self.reset_globals(); 
    }

    fn reset_globals(&self) {
        /* TODO
        if (mainContentContainer.getNamedContent().containsKey("global decl")) {
            final Pointer originalPointer = new Pointer(state.getCurrentPointer());

            choosePath(new Path("global decl"), false);

            // Continue, but without validating external bindings,
            // since we may be doing this reset at initialisation time.
            continueInternal();

            state.setCurrentPointer(originalPointer);
        }

        state.getVariablesState().snapshotDefaultGlobals();
        */
    }

    pub fn build_string_of_hierarchy(&self) -> String {
        let mut sb = String::new();

        self.main_content_container
                .build_string_of_hierarchy(&mut sb, 0, None);// TODO state.getCurrentPointer().resolve());

        sb
    }

    pub fn can_continue(&self) -> bool {
        self.state.can_continue()
    }

    pub fn cont(&self) -> String {
        self.continue_async(0.0);
        self.get_current_text()
    }

    pub fn continue_async(&self, millisecs_limit_async: f32) {
        todo!()
    }

    pub fn get_current_text(&self) -> String {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn oneline_test() {
        let json_string =
            fs::read_to_string("examples/inkfiles/basictext/oneline.ink.json").unwrap();
        let story = Story::new(&json_string).unwrap();
        println!("{}", story.build_string_of_hierarchy());
    }

    #[test]
    fn twolines_test() {
        let json_string =
            fs::read_to_string("examples/inkfiles/basictext/twolines.ink.json").unwrap();
        let story = Story::new(&json_string).unwrap();
        println!("{}", story.build_string_of_hierarchy());
    }

    fn next_all(story: &Story, text: &mut Vec<String>) {
        while story.can_continue() {
            let line = story.cont();
            print!("{line}");

            if !line.trim().is_empty() {
                text.push(line.trim().to_string());
            }
        }

        /* TODO
        if story.has_error() {
            fail(TestUtils.joinText(story.getCurrentErrors()));
        }
        */
    }
}
