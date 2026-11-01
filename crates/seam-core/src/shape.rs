//! A language-neutral description of the data that crosses a boundary.
//!
//! TypeScript interfaces, Pydantic models and OpenAPI schemas are all lowered
//! to shapes so that they can be compared field by field.

use serde::{Deserialize, Serialize};

use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    String,
    Integer,
    Number,
    Boolean,
    Null,
    Array {
        items: Box<Shape>,
    },
    Object(ObjectShape),
    Union {
        variants: Vec<Shape>,
    },
    /// A reference to a named type that has not been resolved yet. The linker
    /// replaces it; if it cannot, it stays and comparisons treat it as unknown.
    Named {
        name: String,
    },
    /// Deliberately untyped (`any`, `Any`, `{}` in a schema).
    Any,
    /// Something the adapter could not lower, with the reason.
    Unknown {
        reason: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectShape {
    /// The declared type name, when there is one.
    pub name: Option<String>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// The name on the wire, after aliases.
    pub json_name: String,
    /// The name in the source, which differs from `json_name` when an alias
    /// is in play.
    pub declared_name: String,
    pub shape: Shape,
    pub required: bool,
    pub nullable: bool,
    pub span: Option<Span>,
}

impl Shape {
    pub fn array(items: Shape) -> Shape {
        Shape::Array {
            items: Box::new(items),
        }
    }

    pub fn unknown(reason: impl Into<String>) -> Shape {
        Shape::Unknown {
            reason: reason.into(),
        }
    }

    /// A short type name for messages: `string`, `integer[]`, `Account`.
    pub fn describe(&self) -> String {
        match self {
            Shape::String => "string".into(),
            Shape::Integer => "integer".into(),
            Shape::Number => "number".into(),
            Shape::Boolean => "boolean".into(),
            Shape::Null => "null".into(),
            Shape::Array { items } => format!("{}[]", items.describe()),
            Shape::Object(object) => object.name.clone().unwrap_or_else(|| "object".into()),
            Shape::Union { variants } => variants
                .iter()
                .map(Shape::describe)
                .collect::<Vec<_>>()
                .join(" | "),
            Shape::Named { name } => name.clone(),
            Shape::Any => "any".into(),
            Shape::Unknown { .. } => "unknown".into(),
        }
    }

    /// Splits `T | null` into `T` and a nullability flag. Other unions are
    /// returned unchanged.
    pub fn without_null(self) -> (Shape, bool) {
        match self {
            Shape::Union { variants } => {
                let had_null = variants.contains(&Shape::Null);
                let mut rest: Vec<Shape> =
                    variants.into_iter().filter(|v| *v != Shape::Null).collect();
                let shape = match rest.len() {
                    0 => Shape::Null,
                    1 => rest.remove(0),
                    _ => Shape::Union { variants: rest },
                };
                (shape, had_null)
            }
            other => (other, false),
        }
    }
}

impl ObjectShape {
    pub fn field(&self, json_name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.json_name == json_name)
    }
}
