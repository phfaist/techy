//! Specifications for macros, environments, and special characters.
//!
//! This module provides the extensibility mechanism for the parser. You can
//! define custom macros, environments, and special characters by creating
//! specifications and adding them to a `ContextDb`.

use std::collections::HashMap;
use std::sync::Arc;

/// Specification for a macro.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroSpec {
    /// The macro name (without backslash).
    pub name: String,
    /// How to parse the macro's arguments.
    pub args_spec: ArgumentStructureSpec,
}

impl MacroSpec {
    /// Create a new macro specification.
    pub fn new(name: impl Into<String>, args_spec: ArgumentStructureSpec) -> Self {
        Self {
            name: name.into(),
            args_spec,
        }
    }

    /// Create a macro with a simple argument spec string.
    ///
    /// The string uses the following syntax:
    /// - `{` = mandatory argument
    /// - `[` = optional argument
    /// - `*` = optional star
    ///
    /// Example: `"*[{"` means optional star, optional argument, mandatory argument.
    pub fn simple(name: impl Into<String>, spec: impl Into<String>) -> Self {
        Self::new(name, ArgumentStructureSpec::parse(spec.into()))
    }
}

/// Specification for an environment.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentSpec {
    /// The environment name.
    pub name: String,
    /// How to parse arguments after \begin{name}.
    pub args_spec: ArgumentStructureSpec,
}

impl EnvironmentSpec {
    /// Create a new environment specification.
    pub fn new(name: impl Into<String>, args_spec: ArgumentStructureSpec) -> Self {
        Self {
            name: name.into(),
            args_spec,
        }
    }

    /// Create an environment with a simple argument spec string.
    pub fn simple(name: impl Into<String>, spec: impl Into<String>) -> Self {
        Self::new(name, ArgumentStructureSpec::parse(spec.into()))
    }
}

/// Specification for special characters.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialsSpec {
    /// The special character(s).
    pub chars: String,
    /// Optional argument specification if this special takes arguments.
    pub args_spec: Option<ArgumentStructureSpec>,
}

impl SpecialsSpec {
    /// Create a new specials specification without arguments.
    pub fn new(chars: impl Into<String>) -> Self {
        Self {
            chars: chars.into(),
            args_spec: None,
        }
    }

    /// Create a specials specification with arguments.
    pub fn with_args(chars: impl Into<String>, args_spec: ArgumentStructureSpec) -> Self {
        Self {
            chars: chars.into(),
            args_spec: Some(args_spec),
        }
    }
}

/// Specification for how to parse arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentStructureSpec {
    /// List of argument specifications.
    pub arguments: Vec<ArgumentSpec>,
}

impl ArgumentStructureSpec {
    /// Create a new arguments specification.
    pub fn new(arguments: Vec<ArgumentSpec>) -> Self {
        Self { arguments }
    }

    /// Create an empty arguments specification.
    pub fn empty() -> Self {
        Self {
            arguments: Vec::new(),
        }
    }

    /// Parse a simple argument spec string.
    ///
    /// - `{` = mandatory argument
    /// - `[` = optional argument  
    /// - `*` = optional star
    /// - `v` = verbatim argument
    pub fn parse(spec: String) -> Self {
        let mut arguments = Vec::new();

        for ch in spec.chars() {
            let arg = match ch {
                '{' => ArgumentSpec::Mandatory,
                '[' => ArgumentSpec::Optional,
                '*' => ArgumentSpec::Star,
                'v' => ArgumentSpec::Verbatim,
                _ => continue, // Ignore unknown characters
            };
            arguments.push(arg);
        }

        Self { arguments }
    }
}

/// A single argument specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentSpec {
    /// A mandatory argument in braces `{...}`.
    Mandatory,
    /// An optional argument in brackets `[...]`.
    Optional,
    /// An optional star `*`.
    Star,
    /// A verbatim argument (like for `\verb|...|`).
    Verbatim,
}

/// A database of known LaTeX constructs.
///
/// This stores all the macro, environment, and specials specifications
/// that the parser knows about.
pub struct ContextDb {
    macros: HashMap<String, Arc<MacroSpec>>,
    environments: HashMap<String, Arc<EnvironmentSpec>>,
    specials: HashMap<String, Arc<SpecialsSpec>>,
}

impl ContextDb {
    /// Create a new empty context database.
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            environments: HashMap::new(),
            specials: HashMap::new(),
        }
    }

    /// Create a context database with standard LaTeX definitions.
    pub fn default() -> Self {
        let mut db = Self::new();
        db.add_standard_definitions();
        db
    }

    /// Add a macro specification.
    pub fn add_macro(&mut self, spec: MacroSpec) {
        self.macros.insert(spec.name.clone(), Arc::new(spec));
    }

    /// Add an environment specification.
    pub fn add_environment(&mut self, spec: EnvironmentSpec) {
        self.environments
            .insert(spec.name.clone(), Arc::new(spec));
    }

    /// Add a specials specification.
    pub fn add_specials(&mut self, spec: SpecialsSpec) {
        self.specials.insert(spec.chars.clone(), Arc::new(spec));
    }

    /// Get a macro specification by name.
    pub fn get_macro(&self, name: &str) -> Option<Arc<MacroSpec>> {
        self.macros.get(name).cloned()
    }

    /// Get an environment specification by name.
    pub fn get_environment(&self, name: &str) -> Option<Arc<EnvironmentSpec>> {
        self.environments.get(name).cloned()
    }

    /// Get a specials specification by characters.
    pub fn get_specials(&self, chars: &str) -> Option<Arc<SpecialsSpec>> {
        self.specials.get(chars).cloned()
    }

    /// Add standard LaTeX macro and environment definitions.
    fn add_standard_definitions(&mut self) {
        // Text formatting macros
        self.add_macro(MacroSpec::simple("textbf", "{"));
        self.add_macro(MacroSpec::simple("textit", "{"));
        self.add_macro(MacroSpec::simple("emph", "{"));
        self.add_macro(MacroSpec::simple("texttt", "{"));
        self.add_macro(MacroSpec::simple("underline", "{"));

        // Sectioning
        self.add_macro(MacroSpec::simple("section", "*[{"));
        self.add_macro(MacroSpec::simple("subsection", "*[{"));
        self.add_macro(MacroSpec::simple("subsubsection", "*[{"));
        self.add_macro(MacroSpec::simple("chapter", "*[{"));
        self.add_macro(MacroSpec::simple("part", "*[{"));

        // References
        self.add_macro(MacroSpec::simple("label", "{"));
        self.add_macro(MacroSpec::simple("ref", "{"));
        self.add_macro(MacroSpec::simple("cite", "[{"));

        // Math
        self.add_macro(MacroSpec::simple("frac", "{{"));
        self.add_macro(MacroSpec::simple("sqrt", "[{"));

        // Environments
        self.add_environment(EnvironmentSpec::simple("equation", ""));
        self.add_environment(EnvironmentSpec::simple("align", ""));
        self.add_environment(EnvironmentSpec::simple("itemize", ""));
        self.add_environment(EnvironmentSpec::simple("enumerate", ""));
        self.add_environment(EnvironmentSpec::simple("center", ""));
        self.add_environment(EnvironmentSpec::simple("quote", ""));

        // Special characters
        self.add_specials(SpecialsSpec::new("&"));
        self.add_specials(SpecialsSpec::new("~"));
        self.add_specials(SpecialsSpec::new("#"));
        self.add_specials(SpecialsSpec::new("``"));
        self.add_specials(SpecialsSpec::new("''"));
    }
}

impl Default for ContextDb {
    fn default() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macro_spec_creation() {
        let spec = MacroSpec::simple("textbf", "{");
        assert_eq!(spec.name, "textbf");
        assert_eq!(spec.args_spec.arguments.len(), 1);
    }

    #[test]
    fn test_arguments_spec_parse() {
        let spec = ArgumentStructureSpec::parse("*[{".to_string());
        assert_eq!(spec.arguments.len(), 3);
        assert_eq!(spec.arguments[0], ArgumentSpec::Star);
        assert_eq!(spec.arguments[1], ArgumentSpec::Optional);
        assert_eq!(spec.arguments[2], ArgumentSpec::Mandatory);
    }

    #[test]
    fn test_context_db() {
        let mut db = ContextDb::new();
        db.add_macro(MacroSpec::simple("test", "{"));

        assert!(db.get_macro("test").is_some());
        assert!(db.get_macro("unknown").is_none());
    }

    #[test]
    fn test_default_context() {
        let db = ContextDb::default();
        assert!(db.get_macro("textbf").is_some());
        assert!(db.get_environment("equation").is_some());
    }
}
