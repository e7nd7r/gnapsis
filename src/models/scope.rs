//! Scope enum representing the compositional hierarchy levels.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Fixed scopes in the compositional hierarchy.
///
/// The hierarchy flows from broadest (Domain) to most specific (Unit):
/// Domain → Feature → Namespace → Component → Unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Scope {
    Domain = 1,
    Feature = 2,
    Namespace = 3,
    Component = 4,
    Unit = 5,
}

impl Scope {
    /// Returns the depth level of this scope (1-5).
    pub fn depth(&self) -> u8 {
        *self as u8
    }

    /// Returns a static slice of all scopes in hierarchical order.
    pub fn all() -> &'static [Scope] {
        &[
            Scope::Domain,
            Scope::Feature,
            Scope::Namespace,
            Scope::Component,
            Scope::Unit,
        ]
    }

    /// Returns a human-readable description of this scope.
    pub fn description(&self) -> &'static str {
        match self {
            Scope::Domain => "Broad business or technical domain",
            Scope::Feature => "Functional capability or feature area",
            Scope::Namespace => "Code module or logical grouping",
            Scope::Component => "Class, struct, trait, or interface",
            Scope::Unit => "Function, method, or property",
        }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Domain" | "domain" => Ok(Scope::Domain),
            "Feature" | "feature" => Ok(Scope::Feature),
            "Namespace" | "namespace" => Ok(Scope::Namespace),
            "Component" | "component" => Ok(Scope::Component),
            "Unit" | "unit" => Ok(Scope::Unit),
            _ => Err(format!(
                "Invalid scope '{}'. Valid values: Domain, Feature, Namespace, Component, Unit",
                s
            )),
        }
    }
}
