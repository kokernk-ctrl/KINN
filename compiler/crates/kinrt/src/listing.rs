//! Disassembler: renders a KIR [`Module`] as human-readable text.
//!
//! Used by the `kinn kir` command and for debugging the lowering pass.

use crate::kir::{Instr, Module};

/// Render the full module listing.
pub fn listing(m: &Module) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "KIR module ({} functions, {} consts)\n",
        m.funcs.len(),
        m.consts.len()
    ));
    // Folded constants (0.4.0 S1): qualified name and value, in the same
    // deterministic order the lowering produced.
    for (name, value) in &m.consts {
        out.push_str(&format!("\nconst {} = {}\n", name, const_value_str(value)));
    }
    for f in &m.funcs {
        out.push_str(&format!(
            "\nfunc {} ({} params, {} locals)\n",
            f.name, f.nparams, f.nlocals
        ));
        for (i, instr) in f.code.iter().enumerate() {
            out.push_str(&format!(
                "  {:04}  {:<18} ; line {}\n",
                i,
                brief(instr),
                f.lines[i]
            ));
        }
    }
    out
}

fn const_value_str(v: &crate::value::Value) -> String {
    match v {
        crate::value::Value::Str(s) => format!("{:?}", s),
        other => other.format(),
    }
}

fn brief(i: &Instr) -> String {
    use crate::value::Value;
    match i {
        Instr::Const(v) => match v {
            Value::Str(s) => format!("Const({:?})", s),
            other => format!("Const({})", other.format()),
        },
        Instr::Load(n) => format!("Load({})", n),
        Instr::Store(n) => format!("Store({})", n),
        Instr::Add => "Add".into(),
        Instr::Sub => "Sub".into(),
        Instr::Mul => "Mul".into(),
        Instr::Div => "Div".into(),
        Instr::Mod => "Mod".into(),
        Instr::Lt => "Lt".into(),
        Instr::Gt => "Gt".into(),
        Instr::Le => "Le".into(),
        Instr::Ge => "Ge".into(),
        Instr::Eq => "Eq".into(),
        Instr::Ne => "Ne".into(),
        Instr::Neg => "Neg".into(),
        Instr::Not => "Not".into(),
        Instr::Jump(t) => format!("Jump({})", t),
        Instr::JumpIfFalse(t) => format!("JumpIfFalse({})", t),
        Instr::Call(f) => format!("Call({})", f),
        Instr::Return => "Return".into(),
        Instr::Pop => "Pop".into(),
        Instr::Print => "Print".into(),
        Instr::ArrNew(n) => format!("ArrNew({})", n),
        Instr::ArrGet => "ArrGet".into(),
        Instr::ArrSet => "ArrSet".into(),
        Instr::Native(id) => format!("Native({})", id.name()),
        Instr::Cast(op) => format!("Cast({})", op.name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kir::FuncDef;
    use crate::value::Value;

    #[test]
    fn listing_shows_functions_and_instructions() {
        let code = vec![Instr::Const(Value::Int(1)), Instr::Print, Instr::Return];
        let m = Module {
            funcs: vec![FuncDef {
                name: "main".to_string(),
                nparams: 0,
                nlocals: 0,
                code,
                lines: vec![1, 2, 3],
            }],
            consts: Vec::new(),
        };
        let text = listing(&m);
        assert!(text.contains("KIR module (1 functions, 0 consts)"));
        assert!(text.contains("func main (0 params, 0 locals)"));
        assert!(text.contains("Const(1)"));
        assert!(text.contains("Print"));
        assert!(text.contains("line 3"));
    }

    #[test]
    fn listing_shows_folded_consts() {
        let m = Module {
            funcs: Vec::new(),
            consts: vec![
                ("MAX".to_string(), Value::Int(10)),
                ("math::PI".to_string(), Value::Float(3.5)),
                ("GREETING".to_string(), Value::Str("hi".to_string())),
            ],
        };
        let text = listing(&m);
        assert!(text.contains("KIR module (0 functions, 3 consts)"));
        assert!(text.contains("const MAX = 10"));
        assert!(text.contains("const math::PI = 3.5"));
        assert!(text.contains("const GREETING = \"hi\""));
    }
}
