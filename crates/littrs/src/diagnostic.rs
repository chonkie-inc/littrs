//! Rich diagnostic error messages inspired by Rust's compiler.
//!
//! This module provides Rust-like error formatting with:
//! - Source code snippets with line numbers
//! - Visual underlines pointing to the error location
//! - Notes providing additional context
//! - Help suggestions showing how to fix the issue

use std::fmt;

/// A span in the source code (byte offsets).
#[derive(Debug, Clone, Copy, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A label attached to a span with a message.
#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            is_primary: true,
        }
    }

    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
            is_primary: false,
        }
    }
}

/// A rich diagnostic error with source context.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The error message (e.g., "type mismatch")
    pub message: String,
    /// The source code that caused the error
    pub source: String,
    /// Labels pointing to specific locations
    pub labels: Vec<Label>,
    /// Additional notes (e.g., "argument `limit` expects an integer")
    pub notes: Vec<String>,
    /// Help suggestions (e.g., "try: search(\"query\", 5)")
    pub help: Vec<String>,
}

impl Diagnostic {
    /// Create a new diagnostic with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: String::new(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: Vec::new(),
        }
    }

    /// Set the source code.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Add a primary label (the main error location).
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::primary(span, message));
        self
    }

    /// Add a secondary label (additional context).
    pub fn with_secondary_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label::secondary(span, message));
        self
    }

    /// Add a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Add a help suggestion.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help.push(help.into());
        self
    }

    /// Find the line and column for a byte offset.
    fn offset_to_line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, ch) in self.source.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Get the line content for a given line number (1-indexed).
    fn get_line(&self, line_num: usize) -> &str {
        self.source.lines().nth(line_num - 1).unwrap_or("")
    }

    /// Calculate the display width needed for line numbers.
    fn line_number_width(&self) -> usize {
        let max_line = self.source.lines().count();
        max_line.to_string().len().max(1)
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Error header
        writeln!(f, "error: {}", self.message)?;

        if self.source.is_empty() || self.labels.is_empty() {
            // No source context, just show notes and help
            for note in &self.notes {
                writeln!(f, "  = note: {}", note)?;
            }
            for help in &self.help {
                writeln!(f, "  = help: {}", help)?;
            }
            return Ok(());
        }

        let width = self.line_number_width();

        // Group labels by line
        let mut labels_by_line: std::collections::BTreeMap<usize, Vec<&Label>> =
            std::collections::BTreeMap::new();

        for label in &self.labels {
            let (line, _) = self.offset_to_line_col(label.span.start);
            labels_by_line.entry(line).or_default().push(label);
        }

        // Print each line with its labels
        writeln!(f, "{:width$} |", "", width = width)?;

        for (&line_num, labels) in &labels_by_line {
            let line_content = self.get_line(line_num);

            // Print the source line
            writeln!(f, "{:width$} | {}", line_num, line_content, width = width)?;

            // Print underlines for each label on this line
            for label in labels {
                let (_, start_col) = self.offset_to_line_col(label.span.start);
                let (_, end_col) = self.offset_to_line_col(label.span.end);

                // Calculate underline position and length
                let underline_start = start_col - 1;
                let underline_len = (end_col - start_col).max(1);

                // Choose underline character based on primary/secondary
                let underline_char = if label.is_primary { '^' } else { '-' };

                // Print the underline
                write!(f, "{:width$} | ", "", width = width)?;
                write!(f, "{:underline_start$}", "")?;
                for _ in 0..underline_len {
                    write!(f, "{}", underline_char)?;
                }

                // Print label message on the same line if it fits
                if !label.message.is_empty() {
                    write!(f, " {}", label.message)?;
                }
                writeln!(f)?;
            }
        }

        writeln!(f, "{:width$} |", "", width = width)?;

        // Print notes
        for note in &self.notes {
            writeln!(f, "  = note: {}", note)?;
        }

        // Print help suggestions
        for help in &self.help {
            writeln!(f, "  = help: {}", help)?;
        }

        Ok(())
    }
}

/// Builder for creating diagnostics in the context of a function call.
#[derive(Debug, Clone)]
pub struct FunctionCallDiagnostic {
    pub func_name: String,
    pub source: String,
    pub call_span: Span,
    pub arg_spans: Vec<Span>,
    pub arg_names: Vec<String>,
    pub expected_types: Vec<String>,
}

impl FunctionCallDiagnostic {
    pub fn new(func_name: impl Into<String>) -> Self {
        Self {
            func_name: func_name.into(),
            source: String::new(),
            call_span: Span::default(),
            arg_spans: Vec::new(),
            arg_names: Vec::new(),
            expected_types: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_call_span(mut self, span: Span) -> Self {
        self.call_span = span;
        self
    }

    pub fn with_arg(mut self, span: Span, name: impl Into<String>, expected_type: impl Into<String>) -> Self {
        self.arg_spans.push(span);
        self.arg_names.push(name.into());
        self.expected_types.push(expected_type.into());
        self
    }

    /// Build a type mismatch diagnostic.
    pub fn type_mismatch(
        self,
        arg_index: usize,
        expected: &str,
        got: &str,
        actual_value: &str,
    ) -> Diagnostic {
        let arg_name = self.arg_names.get(arg_index).map(|s| s.as_str()).unwrap_or("?");
        let arg_span = self.arg_spans.get(arg_index).copied().unwrap_or_default();

        Diagnostic::new(format!("type mismatch in call to `{}`", self.func_name))
            .with_source(self.source)
            .with_label(arg_span, format!("expected `{}`, found `{}`", expected, got))
            .with_note(format!(
                "parameter `{}` of `{}()` expects type `{}`",
                arg_name, self.func_name, expected
            ))
            .with_help(format!(
                "the value `{}` is of type `{}`, but `{}` is required",
                actual_value, got, expected
            ))
    }

    /// Build a missing argument diagnostic.
    pub fn missing_argument(self, arg_name: &str, expected_type: &str) -> Diagnostic {
        Diagnostic::new(format!("missing required argument in call to `{}`", self.func_name))
            .with_source(self.source.clone())
            .with_label(self.call_span, format!("missing `{}`", arg_name))
            .with_note(format!(
                "function signature: {}({})",
                self.func_name,
                self.arg_names
                    .iter()
                    .zip(self.expected_types.iter())
                    .map(|(n, t)| format!("{}: {}", n, t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .with_help(format!(
                "add the missing argument `{}` of type `{}`",
                arg_name, expected_type
            ))
    }

    /// Build an unexpected argument diagnostic.
    pub fn unexpected_argument(self, arg_name: &str, arg_span: Span) -> Diagnostic {
        Diagnostic::new(format!(
            "`{}()` got an unexpected keyword argument `{}`",
            self.func_name, arg_name
        ))
            .with_source(self.source.clone())
            .with_label(arg_span, "unexpected argument")
            .with_note(format!(
                "function signature: {}({})",
                self.func_name,
                self.arg_names
                    .iter()
                    .zip(self.expected_types.iter())
                    .map(|(n, t)| format!("{}: {}", n, t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .with_help(format!(
                "valid arguments are: {}",
                self.arg_names.join(", ")
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_diagnostic() {
        let diag = Diagnostic::new("type mismatch")
            .with_source("search(\"query\", \"five\")")
            .with_label(Span::new(16, 22), "expected `int`, found `str`")
            .with_note("parameter `limit` expects an integer")
            .with_help("try: search(\"query\", 5)");

        let output = diag.to_string();
        assert!(output.contains("error: type mismatch"));
        assert!(output.contains("search(\"query\", \"five\")"));
        assert!(output.contains("^^^^^^"));
        assert!(output.contains("expected `int`, found `str`"));
        assert!(output.contains("note:"));
        assert!(output.contains("help:"));
    }

    #[test]
    fn test_function_call_diagnostic() {
        let diag = FunctionCallDiagnostic::new("search")
            .with_source("search(\"query\", \"five\")")
            .with_call_span(Span::new(0, 23))
            .with_arg(Span::new(7, 14), "query", "str")
            .with_arg(Span::new(16, 22), "limit", "int")
            .type_mismatch(1, "int", "str", "\"five\"");

        let output = diag.to_string();
        assert!(output.contains("type mismatch in call to `search`"));
    }
}
