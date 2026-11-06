//! The sheet model and the clipboard parser: Markdown tables (also psql-style
//! `---+---` output), tab-separated text (what Sheets/Numbers/Excel copy),
//! CSV, and semicolon-separated values.

#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    rows: Vec<Vec<String>>,
}

impl Sheet {
    pub fn parse(text: &str) -> Result<Sheet, String> {
        let text = text.trim_start_matches('\u{feff}');
        if text.trim().is_empty() {
            return Err("The clipboard has no text".into());
        }
        let lines: Vec<&str> = text.lines().collect();
        let rows = if is_markdown(&lines) {
            parse_markdown(&lines)
        } else {
            parse_delimited(text, pick_delimiter(text))?
        };
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if rows.len().saturating_mul(width) > 10_000 {
            return Err("Please paste a smaller table (up to 10,000 cells)".into());
        }
        Sheet::from_rows(rows).ok_or_else(|| "Couldn't find a table on the clipboard".into())
    }

    /// Drops empty rows and pads every row to the same width; preserves CSV values.
    fn from_rows(rows: Vec<Vec<String>>) -> Option<Sheet> {
        let mut rows: Vec<Vec<String>> = rows
            .into_iter()
            .filter(|r| r.iter().any(|c| !c.is_empty()))
            .collect();
        let width = rows.iter().map(Vec::len).max()?;
        for r in &mut rows {
            r.resize(width, String::new());
        }
        Some(Sheet { rows })
    }

    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    pub fn cols(&self) -> usize {
        self.rows.first().map_or(0, Vec::len)
    }

    pub fn get(&self, r: usize, c: usize) -> &str {
        self.rows
            .get(r)
            .and_then(|row| row.get(c))
            .map_or("", String::as_str)
    }

    pub fn set(&mut self, r: usize, c: usize, value: String) {
        if let Some(cell) = self.rows.get_mut(r).and_then(|row| row.get_mut(c)) {
            *cell = value;
        }
    }

    pub fn add_row(&mut self) {
        self.rows.push(vec![String::new(); self.cols()]);
    }

    pub fn add_col(&mut self) {
        for r in &mut self.rows {
            r.push(String::new());
        }
    }
}

/// A delimiter row: `|---|:--:|`, `--- | ---`, or psql's `----+----`.
fn is_separator(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.chars()
            .all(|c| matches!(c, '-' | ':' | '|' | '+' | ' ' | '\t'))
        && t.split(['|', '+'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .all(|p| {
                let p = p.trim_matches(':');
                !p.is_empty() && p.chars().all(|c| c == '-')
            })
}

fn is_markdown(lines: &[&str]) -> bool {
    let has_pipe = |l: &&&str| l.contains('|');
    let separator = lines.iter().any(|l| l.contains('|') && is_separator(l))
        || lines.iter().any(|l| l.contains('+') && is_separator(l));
    let content: Vec<_> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    separator && lines.iter().any(|l| has_pipe(&l))
        || !content.is_empty() && content.iter().all(|l| l.trim_start().starts_with('|'))
}

fn parse_markdown(lines: &[&str]) -> Vec<Vec<String>> {
    lines
        .iter()
        .filter(|l| l.contains('|') && !is_separator(l))
        .map(|l| {
            let t = l.trim();
            let t = t.strip_prefix('|').unwrap_or(t);
            let t = if t.ends_with('|') && !t.ends_with("\\|") {
                &t[..t.len() - 1]
            } else {
                t
            };
            split_pipes(t)
                .into_iter()
                .map(|c| unwrap_inline(c.trim()))
                .collect()
        })
        .collect()
}

/// Splits on `|`, keeping `\|` as a literal pipe.
fn split_pipes(s: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cells.last_mut().unwrap().push('|');
                chars.next();
            }
            '|' => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

/// `` `secret` `` and `**secret**` copy as `secret`.
fn unwrap_inline(s: &str) -> String {
    for wrap in ["``", "`", "**", "__"] {
        if s.len() > 2 * wrap.len() && s.starts_with(wrap) && s.ends_with(wrap) {
            return s[wrap.len()..s.len() - wrap.len()].trim().to_string();
        }
    }
    s.to_string()
}

/// Looks at the first non-empty line, ignoring quoted text.
fn pick_delimiter(text: &str) -> Option<char> {
    let line = text.lines().find(|l| !l.trim().is_empty())?;
    let (mut tabs, mut commas, mut semis, mut quoted) = (0, 0, 0, false);
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            '\t' if !quoted => tabs += 1,
            ',' if !quoted => commas += 1,
            ';' if !quoted => semis += 1,
            _ => {}
        }
    }
    if tabs > 0 {
        Some('\t')
    } else if commas > 0 && commas >= semis {
        Some(',')
    } else if semis > 0 {
        Some(';')
    } else {
        None
    }
}

/// CSV with RFC 4180 quoting (`"a, b"`, `""` for a quote, newlines inside quotes).
/// With no delimiter, every line is a one-cell row.
fn parse_delimited(text: &str, delim: Option<char>) -> Result<Vec<Vec<String>>, String> {
    let Some(delim) = delim else {
        return Ok(text.lines().map(|l| vec![l.to_string()]).collect());
    };
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    cell.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            c if c == delim => row.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    if quoted {
        return Err("The CSV has an unclosed quoted cell".into());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(s: &Sheet) -> Vec<Vec<&str>> {
        (0..s.rows())
            .map(|r| (0..s.cols()).map(|c| s.get(r, c)).collect())
            .collect()
    }

    #[test]
    fn markdown_table() {
        let s = Sheet::parse(
            "Test accounts:\n\n| Role | Email | Password |\n|:-----|------|---:|\n| admin | `a@x.test` | **hunter2** |\n| user | u@x.test | p\\|q |\n",
        )
        .unwrap();
        assert_eq!(
            grid(&s),
            vec![
                vec!["Role", "Email", "Password"],
                vec!["admin", "a@x.test", "hunter2"],
                vec!["user", "u@x.test", "p|q"],
            ]
        );
    }

    #[test]
    fn markdown_without_outer_pipes_and_psql() {
        let s = Sheet::parse("a | b\n--- | ---\n1 | 2").unwrap();
        assert_eq!(grid(&s), vec![vec!["a", "b"], vec!["1", "2"]]);
        let s = Sheet::parse(" id | email\n----+--------\n  1 | a@x\n(1 row)\n").unwrap();
        assert_eq!(grid(&s), vec![vec!["id", "email"], vec!["1", "a@x"]]);
    }

    #[test]
    fn csv_with_quotes() {
        let s =
            Sheet::parse("name,note\r\nann,\"hi, there\"\nbob,\"say \"\"yo\"\"\nok\"\n\n").unwrap();
        assert_eq!(
            grid(&s),
            vec![
                vec!["name", "note"],
                vec!["ann", "hi, there"],
                vec!["bob", "say \"yo\"\nok"]
            ]
        );
    }

    #[test]
    fn tsv_semicolons_ragged_and_single_column() {
        let s = Sheet::parse("a\tb, c\n1\t2").unwrap();
        assert_eq!(grid(&s), vec![vec!["a", "b, c"], vec!["1", "2"]]);
        let s = Sheet::parse("a;b;c\n1;2").unwrap();
        assert_eq!(grid(&s), vec![vec!["a", "b", "c"], vec!["1", "2", ""]]);
        let s = Sheet::parse("one\ntwo\n").unwrap();
        assert_eq!(grid(&s), vec![vec!["one"], vec!["two"]]);
    }

    #[test]
    fn preserves_csv_spaces_and_rejects_broken_quotes() {
        let s = Sheet::parse("email,password\na@x,\" secret \" ").unwrap();
        assert_eq!(s.get(1, 1), " secret  ");
        assert!(Sheet::parse("a,b\n1,\"broken").is_err());
        assert!(Sheet::parse(&"a,b\n".repeat(5001)).is_err());
    }

    #[test]
    fn rejects_empty_and_edits() {
        assert!(Sheet::parse("  \n ").is_err());
        let mut s = Sheet::parse("a,b").unwrap();
        s.add_row();
        s.add_col();
        s.set(1, 2, "z".into());
        assert_eq!(grid(&s), vec![vec!["a", "b", ""], vec!["", "", "z"]]);
    }
}
