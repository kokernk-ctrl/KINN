//! Disassembler: renders a KIR [`Module`] as human-readable text.
//!
//! Used by the `kinn kir` command and for debugging the lowering pass.

use crate::kir::{Instr, Module};

/// Render the full module listing.
pub fn listing(m: &Module) -> String {
    let mut out = String::new();
    out.push_str(&format!("KIR module ({} functions)\n", m.funcs.len()));
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
        };
        let text = listing(&m);
        assert!(text.contains("KIR module (1 functions)"));
        assert!(text.contains("func main (0 params, 0 locals)"));
        assert!(text.contains("Const(1)"));
        assert!(text.contains("Print"));
        assert!(text.contains("line 3"));
    }
}
