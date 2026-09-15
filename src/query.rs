use anyhow::{bail, Result};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchOptions {
    pub match_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub name_terms: Vec<String>,
    pub path_terms: Vec<String>,
    pub match_path: bool,
}

pub fn has_wildcards(term: &str) -> bool {
    term.contains('*') || term.contains('?')
}

pub fn parse(input: &str, options: SearchOptions) -> Result<SearchQuery> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.chars() {
        match character {
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            value => current.push(value),
        }
    }
    if quoted {
        bail!("unterminated quoted search term");
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut name_terms = Vec::new();
    let mut path_terms = Vec::new();
    for token in tokens {
        if let Some(path) = token.strip_prefix("path:") {
            if path.is_empty() {
                bail!("path: requires a path fragment");
            }
            path_terms.push(path.to_lowercase());
        } else {
            name_terms.push(token.to_lowercase());
        }
    }
    Ok(SearchQuery {
        name_terms,
        path_terms,
        match_path: options.match_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_paths_and_name_terms() {
        let query = parse(
            "path:\"My Projects\" annual report",
            SearchOptions::default(),
        )
        .unwrap();
        assert_eq!(query.path_terms, ["my projects"]);
        assert_eq!(query.name_terms, ["annual", "report"]);
    }

    #[test]
    fn rejects_empty_and_unterminated_path_terms() {
        assert!(parse("path:", SearchOptions::default()).is_err());
        assert!(parse("path:\"unfinished", SearchOptions::default()).is_err());
    }
}
