use std::{collections::HashMap, io::Write, rc::Rc};

use crate::{
    choice::Choice,
    choice_point::ChoicePoint,
    container::Container,
    control_command::ControlCommand,
    divert::Divert,
    glue::Glue,
    ink_list::InkList,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    path::Path,
    push_pop::PushPopType,
    story_error::StoryError,
    tag::Tag,
    value::Value,
    value_type::{StringValue, VariablePointerValue},
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};

use super::json_writer::JsonWriter;

pub(crate) fn write_dictionary_values<W: Write>(
    writer: &mut JsonWriter<W>,
    values: &HashMap<String, Rc<Value>>,
) -> Result<(), StoryError> {
    writer.raw("{")?;
    let mut first = true;
    let mut entries: Vec<_> = values.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (key, value) in entries {
        writer.key(&mut first, key)?;
        write_rtobject(writer, value.clone())?;
    }
    writer.raw("}")?;
    Ok(())
}

pub(crate) fn write_rtobject<W: Write>(
    writer: &mut JsonWriter<W>,
    object: Rc<dyn RTObject>,
) -> Result<(), StoryError> {
    if let Some(container) = object.as_any().downcast_ref::<Container>() {
        return write_rt_container(writer, container, false);
    }
    if let Ok(divert) = object.clone().into_any().downcast::<Divert>() {
        writer.raw("{")?;
        let mut first = true;
        let key = if divert.is_external {
            "x()"
        } else if divert.pushes_to_stack && divert.stack_push_type == PushPopType::Function {
            "f()"
        } else if divert.pushes_to_stack && divert.stack_push_type == PushPopType::Tunnel {
            "->t->"
        } else {
            "->"
        };
        writer.key(&mut first, key)?;
        let target = if let Some(name) = &divert.variable_divert_name {
            name.clone()
        } else {
            divert
                .get_target_path_string()
                .ok_or_else(|| StoryError::InvalidStoryState("divert has no target".to_owned()))?
        };
        writer.string(&target)?;
        if divert.has_variable_target() {
            writer.key(&mut first, "var")?;
            writer.boolean(true)?;
        }
        if divert.is_conditional {
            writer.key(&mut first, "c")?;
            writer.boolean(true)?;
        }
        if divert.external_args > 0 {
            writer.key(&mut first, "exArgs")?;
            writer.integer(divert.external_args)?;
        }
        writer.raw("}")?;
        return Ok(());
    }
    if let Ok(choice_point) = object.clone().into_any().downcast::<ChoicePoint>() {
        writer.raw("{")?;
        let mut first = true;
        writer.key(&mut first, "*")?;
        writer.string(&choice_point.get_path_string_on_choice())?;
        writer.key(&mut first, "flg")?;
        writer.integer(choice_point.get_flags())?;
        writer.raw("}")?;
        return Ok(());
    }
    if let Some(value) = Value::get_bool_value(object.as_ref()) {
        writer.boolean(value)?;
        return Ok(());
    }
    if let Some(value) = Value::get_value::<i32>(object.as_ref()) {
        writer.integer(value)?;
        return Ok(());
    }
    if let Some(value) = Value::get_value::<f32>(object.as_ref()) {
        writer.float(value)?;
        return Ok(());
    }
    if let Some(value) = Value::get_value::<&StringValue>(object.as_ref()) {
        let encoded = if value.is_newline {
            "\n".to_owned()
        } else {
            format!("^{}", value.string)
        };
        writer.string(&encoded)?;
        return Ok(());
    }
    if let Some(value) = Value::get_value::<&InkList>(object.as_ref()) {
        return write_ink_list(writer, value);
    }
    if let Some(value) = Value::get_value::<&Path>(object.as_ref()) {
        writer.raw("{\"^->\":")?;
        writer.string(&value.get_components_string())?;
        writer.raw("}")?;
        return Ok(());
    }
    if let Some(value) = Value::get_value::<&VariablePointerValue>(object.as_ref()) {
        writer.raw("{\"^var\":")?;
        writer.string(&value.variable_name)?;
        writer.raw(",\"ci\":")?;
        writer.integer(value.context_index)?;
        writer.raw("}")?;
        return Ok(());
    }
    if object.as_any().is::<Glue>() {
        writer.string("<>")?;
        return Ok(());
    }
    if let Some(command) = object.as_any().downcast_ref::<ControlCommand>() {
        writer.string(&ControlCommand::get_name(command.command_type))?;
        return Ok(());
    }
    if let Some(function) = object.as_any().downcast_ref::<NativeFunctionCall>() {
        let mut name = NativeFunctionCall::get_name(function.op);
        if name == "^" {
            name = "L^".to_owned();
        }
        writer.string(&name)?;
        return Ok(());
    }
    if let Ok(reference) = object.clone().into_any().downcast::<VariableReference>() {
        writer.raw("{")?;
        if let Some(path) = reference.get_path_string_for_count() {
            writer.raw("\"CNT?\":")?;
            writer.string(&path)?;
        } else {
            writer.raw("\"VAR?\":")?;
            writer.string(&reference.name)?;
        }
        writer.raw("}")?;
        return Ok(());
    }
    if let Some(assignment) = object.as_any().downcast_ref::<VariableAssignment>() {
        writer.raw("{")?;
        writer.string(if assignment.is_global {
            "VAR="
        } else {
            "temp="
        })?;
        writer.raw(":")?;
        writer.string(&assignment.variable_name)?;
        if !assignment.is_new_declaration {
            writer.raw(",\"re\":true")?;
        }
        writer.raw("}")?;
        return Ok(());
    }
    if object.as_any().is::<Void>() {
        writer.string("void")?;
        return Ok(());
    }
    if let Some(tag) = object.as_any().downcast_ref::<Tag>() {
        writer.raw("{\"#\":")?;
        writer.string(tag.get_text())?;
        writer.raw("}")?;
        return Ok(());
    }
    if let Some(choice) = object.as_any().downcast_ref::<Choice>() {
        return write_choice(writer, choice);
    }
    Err(StoryError::BadJson(format!(
        "Failed to write runtime object to JSON: {object}"
    )))
}

pub(crate) fn write_rt_container<W: Write>(
    writer: &mut JsonWriter<W>,
    container: &Container,
    without_name: bool,
) -> Result<(), StoryError> {
    writer.raw("[")?;
    let mut first = true;
    for child in &container.content {
        writer.separator(&mut first)?;
        write_rtobject(writer, child.clone())?;
    }
    writer.separator(&mut first)?;
    let named = container.get_named_only_content();
    let flags = container.get_count_flags();
    let has_name = container.name.is_some() && !without_name;
    if named.is_empty() && flags == 0 && !has_name {
        writer.raw("null")?;
    } else {
        writer.raw("{")?;
        let mut first_property = true;
        let mut entries: Vec<_> = named.iter().collect();
        entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
        for (name, child) in entries {
            writer.key(&mut first_property, name)?;
            write_rt_container(writer, child, true)?;
        }
        if flags > 0 {
            writer.key(&mut first_property, "#f")?;
            writer.integer(flags)?;
        }
        if let Some(name) = container.name.as_ref().filter(|_| has_name) {
            writer.key(&mut first_property, "#n")?;
            writer.string(name)?;
        }
        writer.raw("}")?;
    }
    writer.raw("]")?;
    Ok(())
}

fn write_ink_list<W: Write>(writer: &mut JsonWriter<W>, list: &InkList) -> Result<(), StoryError> {
    writer.raw("{\"list\":{")?;
    let mut entries: Vec<_> = list.items.iter().collect();
    entries.sort_unstable_by_key(|(item, _)| item.get_full_name());
    let mut first = true;
    for (item, value) in entries {
        writer.separator(&mut first)?;
        writer.string(&item.get_full_name())?;
        writer.raw(":")?;
        writer.integer(value)?;
    }
    writer.raw("}}")?;
    Ok(())
}

pub(crate) fn write_choice<W: Write>(
    writer: &mut JsonWriter<W>,
    choice: &Choice,
) -> Result<(), StoryError> {
    writer.raw("{")?;
    let mut first = true;
    writer.key(&mut first, "text")?;
    writer.string(&choice.text)?;
    writer.key(&mut first, "index")?;
    writer.integer(*choice.index.borrow())?;
    writer.key(&mut first, "originalChoicePath")?;
    writer.string(&choice.source_path)?;
    writer.key(&mut first, "originalThreadIndex")?;
    writer.integer(*choice.original_thread_index.borrow())?;
    writer.key(&mut first, "targetPath")?;
    writer.string(&choice.target_path.to_string())?;
    writer.key(&mut first, "tags")?;
    writer.raw("[")?;
    let mut first_tag = true;
    for tag in &choice.tags {
        writer.separator(&mut first_tag)?;
        writer.string(tag)?;
    }
    writer.raw("]}")?;
    Ok(())
}

pub(crate) fn write_list_rt_objs<W: Write>(
    writer: &mut JsonWriter<W>,
    objects: &[Rc<dyn RTObject>],
) -> Result<(), StoryError> {
    writer.raw("[")?;
    let mut first = true;
    for object in objects {
        writer.separator(&mut first)?;
        write_rtobject(writer, object.clone())?;
    }
    writer.raw("]")?;
    Ok(())
}

pub(crate) fn write_int_dictionary<W: Write>(
    writer: &mut JsonWriter<W>,
    values: &HashMap<String, i32>,
) -> Result<(), StoryError> {
    writer.raw("{")?;
    let mut first = true;
    let mut entries: Vec<_> = values.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (key, value) in entries {
        writer.key(&mut first, key)?;
        writer.integer(value)?;
    }
    writer.raw("}")?;
    Ok(())
}
