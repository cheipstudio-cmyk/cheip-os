//! Calculator: keeps the typed expression and evaluates it with the usual
//! precedence (× ÷ before + −), like the Android and iOS calculators.

#[derive(Default, Clone)]
pub struct Calc {
    /// What the user typed, with the display symbols (× ÷ −)
    pub expr: String,
    /// Live preview of the result, or the final result after "="
    pub result: String,
    /// "=" was pressed: the next digit starts a new expression
    done: bool,
}

fn is_op(c: char) -> bool {
    matches!(c, '+' | '−' | '×' | '÷')
}

impl Calc {
    pub fn key(&mut self, k: &str) {
        match k {
            "AC" => *self = Calc::default(),
            "⌫" => {
                if self.done {
                    *self = Calc::default();
                } else {
                    self.expr.pop();
                }
            }
            "=" => {
                if let Some(v) = eval(&self.expr) {
                    self.result = format_num(v);
                    self.expr = self.result.clone();
                    self.done = true;
                    return;
                } else if !self.expr.is_empty() {
                    self.result = "Error".into();
                    return;
                }
            }
            "±" => self.negate(),
            "%" => {
                // Percent of the last number: 50% -> 0.5
                let (head, num) = self.split_last();
                if let Ok(v) = num.parse::<f64>() {
                    self.expr = format!("{head}{}", format_num(v / 100.0));
                }
                self.done = false;
            }
            "+" | "−" | "×" | "÷" => {
                self.done = false;
                let op = k.chars().next().unwrap();
                match self.expr.chars().last() {
                    None => {
                        if op == '−' {
                            self.expr.push('−');
                        }
                    }
                    // Replace an operator typed twice
                    Some(c) if is_op(c) => {
                        if self.expr.chars().count() > 1 {
                            self.expr.pop();
                            self.expr.push(op);
                        }
                    }
                    Some('.') => {
                        self.expr.pop();
                        self.expr.push(op);
                    }
                    Some(_) => self.expr.push(op),
                }
            }
            "." => {
                if self.done {
                    *self = Calc::default();
                }
                let (_, num) = self.split_last();
                if !num.contains('.') {
                    if num.is_empty() || num == "−" {
                        self.expr.push('0');
                    }
                    self.expr.push('.');
                }
            }
            d if d.len() == 1 && d.chars().all(|c| c.is_ascii_digit()) => {
                if self.done {
                    *self = Calc::default();
                }
                let (_, num) = self.split_last();
                // No leading zeros like "007"
                if num == "0" {
                    self.expr.pop();
                } else if num == "−0" {
                    self.expr.pop();
                }
                if self.expr.chars().count() < 40 {
                    self.expr.push_str(d);
                }
            }
            _ => {}
        }
        self.preview();
    }

    /// Splits "12+3.5" into ("12+", "3.5")
    fn split_last(&self) -> (String, String) {
        let chars: Vec<char> = self.expr.chars().collect();
        let mut i = chars.len();
        while i > 0 && !is_op(chars[i - 1]) {
            i -= 1;
        }
        // A minus at the start, or right after another operator, is a sign
        if i > 0 && chars[i - 1] == '−' && (i == 1 || is_op(chars[i - 2])) {
            i -= 1;
        }
        (chars[..i].iter().collect(), chars[i..].iter().collect())
    }

    fn negate(&mut self) {
        let (head, num) = self.split_last();
        if num.is_empty() {
            return;
        }
        self.expr = match num.strip_prefix('−') {
            Some(rest) => format!("{head}{rest}"),
            None => format!("{head}−{num}"),
        };
        self.done = false;
    }

    fn preview(&mut self) {
        self.result = match eval(&self.expr) {
            // Show a preview only once there is an operation to preview
            Some(v) if self.expr.chars().skip(1).any(is_op) => format_num(v),
            _ => String::new(),
        };
    }
}

/// Evaluates "12+3×4" → 24. None when incomplete or invalid (division by zero).
pub fn eval(expr: &str) -> Option<f64> {
    let mut nums: Vec<f64> = Vec::new();
    let mut ops: Vec<char> = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Number, with an optional leading sign
        let mut s = String::new();
        if chars[i] == '−' {
            s.push('-');
            i += 1;
        }
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            s.push(chars[i]);
            i += 1;
        }
        nums.push(s.parse::<f64>().ok()?);
        if i < chars.len() {
            if !is_op(chars[i]) {
                return None;
            }
            ops.push(chars[i]);
            i += 1;
        }
    }
    if nums.is_empty() || ops.len() != nums.len() - 1 {
        return None;
    }
    // × and ÷ first
    let mut n2 = vec![nums[0]];
    let mut o2 = Vec::new();
    for (k, op) in ops.iter().enumerate() {
        let b = nums[k + 1];
        match op {
            '×' => *n2.last_mut()? *= b,
            '÷' => {
                if b == 0.0 {
                    return None;
                }
                *n2.last_mut()? /= b;
            }
            _ => {
                o2.push(*op);
                n2.push(b);
            }
        }
    }
    let mut v = n2[0];
    for (k, op) in o2.iter().enumerate() {
        if *op == '+' {
            v += n2[k + 1];
        } else {
            v -= n2[k + 1];
        }
    }
    v.is_finite().then_some(v)
}

/// 0.1+0.2 shows "0.3", big and tiny numbers in scientific notation
pub fn format_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    let s = if !(1e-9..1e15).contains(&a) {
        let s = format!("{v:.6e}");
        // 1.500000e3 -> 1.5e3
        match s.split_once('e') {
            Some((m, e)) => {
                let m = m.trim_end_matches('0').trim_end_matches('.');
                format!("{m}e{e}")
            }
            None => s,
        }
    } else {
        let s = format!("{v:.10}");
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" { "0".into() } else { s }
    };
    s.replace('-', "−")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(keys: &[&str]) -> Calc {
        let mut c = Calc::default();
        for k in keys {
            c.key(k);
        }
        c
    }

    #[test]
    fn precedence() {
        assert_eq!(eval("2+3×4"), Some(14.0));
        assert_eq!(eval("10−4÷2"), Some(8.0));
        assert_eq!(eval("−3×−2"), Some(6.0));
        assert_eq!(eval("1÷0"), None);
        assert_eq!(eval("2+"), None);
    }

    #[test]
    fn typing() {
        let c = run(&["1", "2", "+", "3", "="]);
        assert_eq!(c.result, "15");
        let c = run(&["0", ".", "1", "+", "0", ".", "2", "="]);
        assert_eq!(c.result, "0.3");
        let c = run(&["5", "+", "×", "2"]);
        assert_eq!(c.expr, "5×2");
        assert_eq!(c.result, "10");
        let c = run(&["0", "0", "7"]);
        assert_eq!(c.expr, "7");
        let c = run(&["1", "÷", "0", "="]);
        assert_eq!(c.result, "Error");
    }

    #[test]
    fn sign_and_percent() {
        let c = run(&["5", "±"]);
        assert_eq!(c.expr, "−5");
        let c = run(&["2", "0", "0", "×", "5", "0", "%", "="]);
        assert_eq!(c.result, "100");
        let c = run(&["9", "−", "3", "±", "="]);
        assert_eq!(c.result, "12");
    }

    #[test]
    fn after_equals() {
        let c = run(&["2", "×", "3", "=", "+", "1", "="]);
        assert_eq!(c.result, "7");
        let c = run(&["2", "×", "3", "=", "4"]);
        assert_eq!(c.expr, "4");
    }

    #[test]
    fn formatting() {
        assert_eq!(format_num(1.0 / 3.0), "0.3333333333");
        assert_eq!(format_num(-2.5), "−2.5");
        assert_eq!(format_num(1e20), "1e20");
    }
}
