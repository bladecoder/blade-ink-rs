use std::{
    fmt,
    hash::{Hash, Hasher}, cell::OnceCell,
};

const PARENT_ID: &str = "^";


/// The componentsString field from the C# impl. has been removed and it is always generated dinamically from the components field.
#[derive(Eq, Clone, Default)]
pub struct Path {
    components: Vec<Component>,
    is_relative: bool,
    components_string: OnceCell<String>,
}

impl Path {
    pub fn new(components: &[Component], relative: bool) -> Path {
        let mut comp: Vec<Component> = Vec::new();
        comp.extend_from_slice(components);
        Path {
            components: comp,
            is_relative: relative,
            ..Default::default()
        }
    }

    pub fn new_with_defaults() -> Path {
        Path {
            ..Default::default()
        }
    }

    pub fn new_with_components_string(components_string: Option<&str>) -> Path {
        let cs = components_string;
        let is_relative:bool;

        // Empty path, empty components
        // (path is to root, like "/" in file system)
        if cs.is_none() || cs.as_ref().unwrap().is_empty() {
            return Path {
                ..Default::default()
            };
        }

        let mut cs = cs.unwrap().to_string();

        // When components start with ".", it indicates a relative path, e.g.
        // .^.^.hello.5
        // is equivalent to file system style path:
        // ../../hello/5

        if cs.starts_with('.') {
            is_relative = true;
            cs = cs[1..].to_string();
        } else {
            is_relative = false;
        }

        let component_string = cs.split('.');
        let mut components = Vec::new();

        for str in component_string {
            let index = str.parse::<usize>();

            match index {
                Ok(index) => components.push(Component::new_i(index)),
                Err(_) => components.push(Component::new(str)),
            }
        }

        let cs_cell = OnceCell::new();
        let _ = cs_cell.set(cs);

        Path {
            components,
            is_relative,
            components_string: cs_cell
        }
    }

    pub fn get_component(&self, index: usize) -> Option<&Component> {
        self.components.get(index)
    }

    pub fn is_relative(&self) -> bool {
        self.is_relative
    }

    pub fn get_tail(&self) -> Path {
        if self.components.len() >= 2 {
            let tail_comps = &self.components[1..];

            Path::new(tail_comps, false)
        } else {
            Path::get_self()
        }
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn get_self() -> Path {
        Path {
            is_relative: true,
            ..Default::default()
        }
    }

    pub fn get_last_component(&self) -> Option<&Component> {
        if !self.components.is_empty() {
            return self.components.last();
        }

        None
    }

    pub fn path_by_appending_path(&self, path_to_append: &Path) -> Path {
        let mut upward_moves = 0;

        for component in path_to_append.components.iter() {
            if component.is_parent() {
                upward_moves += 1;
            } else {
                break;
            }
        }

        let mut components = Vec::new();

        // TODO check that this is correct
        for i in 0..self.components.len() - upward_moves {
            components.push(self.components.get(i).unwrap().clone());
        }

        for i in upward_moves..self.components.len() {
            components.push(self.components.get(i).unwrap().clone());
        }

        Path {
            components,
            ..Default::default()
        }
    }

    pub fn get_components_string(&self) -> String {
        return self.components_string.get_or_init( || {
            let mut sb = String::new();

            if !self.components.is_empty() {
                sb.push_str(&self.components.get(0).unwrap().to_string());

                for i in 1..self.components.len() {
                    sb.push('.');
                    sb.push_str(&self.components.get(i).unwrap().to_string());
                }
            }

            if self.is_relative {
                return ".".to_owned() + &sb;
            }

            sb
        }).to_string();
    }

    pub fn path_by_appending_component( &self, c: Component) -> Path {
        let mut p = Path::new(self.components.as_ref(), false);
        p.components.push(c);

        p 
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_components_string())
    }
}

impl Hash for Path {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_string().hash(state)
    }
}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        if other.components.len() != self.components.len() {
            return false;
        }

        if other.is_relative() != self.is_relative() {
            return false;
        }

        for i in 0..other.components.len() {
            if !other
                .components
                .get(i)
                .unwrap()
                .eq(self.components.get(i).unwrap())
            {
                return false;
            }
        }

        true
    }
}

#[derive(Eq, Clone)]
pub struct Component {
    pub index: Option<usize>,
    pub name: Option<String>,
}

impl Component {
    pub fn new(name: &str) -> Component {
        Component {
            name: Some(name.to_string()),
            index: None,
        }
    }

    pub fn new_i(index: usize) -> Component {
        Component {
            name: None,
            index: Some(index),
        }
    }

    pub fn to_parent() -> Component {
        Component::new(PARENT_ID)
    }

    pub fn is_index(&self) -> bool {
        self.index.is_some()
    }

    pub fn is_parent(&self) -> bool {
        match &self.name {
            Some(name) => name.eq(PARENT_ID),
            None => false,
        }
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self.index {
            Some(index) => index.to_string(),
            None => self.name.as_ref().unwrap().to_string(),
        };

        write!(f, "{s}")
    }
}

impl PartialEq for Component {
    fn eq(&self, other: &Self) -> bool {
        if other.is_index() == self.is_index() {
            match self.index {
                Some(index) => return index == other.index.unwrap(),
                None => return self.name.as_ref().unwrap().eq(other.name.as_ref().unwrap()),
            }
        }

        false
    }
}

impl Hash for Component {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.index {
            Some(index) => index.hash(state),
            None => return self.name.as_ref().unwrap().hash(state),
        }
    }
}
