//! Runtime values manipulated by the KIR virtual machine.

/// A KIR value.
///
/// The type checker guarantees that only well-typed combinations of values
/// ever reach an instruction, so mismatched operand pairs are reported as
/// internal errors rather than user-facing diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    /// Placeholder for slots that have not been written yet and for `void`
    /// function results. User programs can never observe a `Unit`: the
    /// checker rejects void values in every consuming position.
    Unit,
}

impl Value {
    /// Name of the Kin type this value belongs to.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Unit => "void",
        }
    }

    /// Human-oriented formatting used by `print`.
    ///
    /// Integral floats keep a decimal point (`2.0`) so the type stays
    /// visible to beginners; all other floats use the shortest
    /// representation that round-trips.
    pub fn format(&self) -> String {
        match self {
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_finite() && *f == f.trunc() && f.abs() < 1e16 {
                    format!("{:.1}", f)
                } else {
                    format!("{}", f)
                }
            }
            Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            Value::Str(s) => s.clone(),
            Value::Unit => "()".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn formats_ints_and_bools() {
        assert_eq!(Value::Int(-42).format(), "-42");
        assert_eq!(Value::Bool(true).format(), "true");
        assert_eq!(Value::Bool(false).format(), "false");
    }

    #[test]
    fn integral_floats_keep_decimal_point() {
        assert_eq!(Value::Float(2.0).format(), "2.0");
        assert_eq!(Value::Float(-0.5).format(), "-0.5");
        assert_eq!(Value::Float(3.25).format(), "3.25");
        // 0.1 + 0.2 must print the honest shortest round-trip form.
        assert_eq!(Value::Float(0.1 + 0.2).format(), "0.30000000000000004");
    }

    #[test]
    fn type_names_match_kin_types() {
        assert_eq!(Value::Int(0).type_name(), "int");
        assert_eq!(Value::Float(0.0).type_name(), "float");
        assert_eq!(Value::Bool(false).type_name(), "bool");
        assert_eq!(Value::Str(String::new()).type_name(), "string");
        assert_eq!(Value::Unit.type_name(), "void");
    }
}
