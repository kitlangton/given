use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WithSpan<T> {
    pub(crate) value: T,
    pub(crate) position: Span,
}

impl<T> WithSpan<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> WithSpan<U> {
        WithSpan {
            value: f(self.value),
            position: self.position.clone(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for WithSpan<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

pub struct Edit {
    pub span: Span,
    pub text: String,
}

impl Edit {
    pub fn apply_edits(edits: Vec<Edit>, original: &str) -> String {
        let mut result = String::new();
        let mut last_index = 0;
        let mut sorted_edits = edits;

        // Sort edits by start position
        sorted_edits.sort_by_key(|e| e.span.start);

        let mut previous_end = 0;

        for edit in sorted_edits {
            // Skip overlapping edits
            if edit.span.start < previous_end {
                continue;
            }

            // Append the part of the original string before the current edit
            result.push_str(&original[last_index..edit.span.start]);
            // Append the edit text
            result.push_str(&edit.text);
            // Update the last index to the end of the current edit
            last_index = edit.span.end;
            // Update the previous end to the end of the current edit
            previous_end = edit.span.end;
        }

        // Append the remaining part of the original string
        result.push_str(&original[last_index..]);

        result
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_apply_edits() {
        let original = "Hello, world!";
        let edits = vec![
            Edit {
                span: Span::new(7, 12),
                text: "Rust".to_string(),
            },
            Edit {
                span: Span::new(0, 5),
                text: "Hi".to_string(),
            },
        ];
        let result = Edit::apply_edits(edits, original);
        assert_eq!(result, "Hi, Rust!");
    }
}
