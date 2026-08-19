use std::path::Path;

const WEAK: &[&str] = &[
    "main", "app", "index", "init", "setup", "test", "tests", "spec", "utils", "util", "helpers",
    "types", "config", "common", "shared",
];

fn identifier_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?.trim_start();
    let name = rest
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .next()?;
    (!name.is_empty()).then_some(name)
}

fn go_symbol(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("func ") {
        let rest = rest.trim_start();
        let rest = if rest.starts_with('(') {
            rest.split_once(')')?.1.trim_start()
        } else {
            rest
        };
        return rest.split_once('(').map(|(name, _)| name.trim());
    }
    identifier_after(line, "type ")
}

fn typescript_symbol(line: &str) -> Option<&str> {
    let mut line = line.trim_start();
    for prefix in ["export default ", "export ", "declare ", "async "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            line = rest.trim_start();
        }
    }
    for prefix in [
        "async function ",
        "function ",
        "class ",
        "interface ",
        "enum ",
        "type ",
        "const ",
        "let ",
    ] {
        if let Some(name) = identifier_after(line, prefix) {
            return Some(name);
        }
    }
    None
}

fn strong(symbol: &str) -> bool {
    symbol.len() >= 4 && !WEAK.contains(&symbol.to_ascii_lowercase().as_str())
}

pub(crate) fn proving_question(
    files: impl IntoIterator<Item = (String, String)>,
) -> Option<String> {
    for (path, content) in files {
        let extension = Path::new(&path)
            .extension()
            .and_then(|value| value.to_str());
        let parser: fn(&str) -> Option<&str> = match extension {
            Some("go") => go_symbol,
            Some("ts" | "tsx") => typescript_symbol,
            _ => continue,
        };
        for line in content.lines() {
            if let Some(symbol) = parser(line).filter(|symbol| strong(symbol)) {
                return Some(format!("What does `{symbol}` do, and who calls it?"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_question_names_a_symbol_the_customer_wrote() {
        let question = proving_question([(
            "src/retry.ts".to_string(),
            "export class RetryScheduler { run() {} }\n".to_string(),
        )])
        .expect("question");
        assert!(question.contains("RetryScheduler"));
        assert!(!question.contains("retry.ts"));
    }

    #[test]
    fn go_question_names_a_method_after_its_receiver() {
        let question = proving_question([(
            "worker/dispatch.go".to_string(),
            "func (w *Worker) DispatchEnvelope(ctx context.Context) error { return nil }\n"
                .to_string(),
        )])
        .expect("question");
        assert!(question.contains("DispatchEnvelope"));
    }

    #[test]
    fn generic_or_unparseable_files_do_not_fabricate_a_question() {
        assert_eq!(
            proving_question([
                (
                    "src/main.ts".to_string(),
                    "export function main() {}\n".to_string(),
                ),
                ("README.md".to_string(), "RetryScheduler\n".to_string()),
            ]),
            None
        );
    }

    #[test]
    fn a_mixed_repo_uses_a_real_symbol_from_either_language() {
        let question = proving_question([
            (
                "src/index.ts".to_string(),
                "export const config = {};\n".to_string(),
            ),
            (
                "worker/queue.go".to_string(),
                "type DeliveryQueue struct {}\n".to_string(),
            ),
        ])
        .expect("question");
        assert!(question.contains("DeliveryQueue"));
    }
}
