use std::collections::HashMap;

use rust_stemmers::Stemmer;
use stopwords::Stopwords;

#[derive(Debug, Clone, PartialEq)]
pub struct Lexeme {
    pub term: String,
    pub positions: Vec<u32>,
    pub weight: char,
}

#[derive(Debug, PartialEq)]
pub struct TSVector {
    pub lexemes: Vec<Lexeme>,
}

impl TSVector {
    pub fn new() -> Self {
        TSVector {
            lexemes: Vec::new(),
        }
    }
}

impl std::fmt::Display for TSVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let lexeme_strings: Vec<String> = self
            .lexemes
            .iter()
            .map(|lexeme| {
                let positions_str = lexeme
                    .positions
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<String>>()
                    .join(",");
                format!("'{}':{}", lexeme.term, positions_str)
            })
            .collect();

        write!(f, "{}", lexeme_strings.join(" "))
    }
}

pub fn to_tsvector(text: &str) -> TSVector {
    to_tsvector_weighted(text, 'D')
}

pub fn to_tsvector_weighted(text: &str, default_weight: char) -> TSVector {
    let mut lexeme_map: HashMap<String, Vec<u32>> = HashMap::new();
    let mut position = 1u32;

    let tokens = tokenize(text);

    for token in tokens {
        let normalized = normalize_token(&token);
        if !normalized.is_empty() && !is_stopword(&normalized) {
            lexeme_map
                .entry(normalized)
                .or_insert_with(Vec::new)
                .push(position);
        }
        position += 1;
    }

    let mut lexemes: Vec<Lexeme> = lexeme_map
        .into_iter()
        .map(|(term, mut positions)| {
            positions.sort_unstable();
            Lexeme {
                term,
                positions,
                weight: default_weight,
            }
        })
        .collect();

    lexemes.sort_by(|a, b| a.term.cmp(&b.term));

    TSVector { lexemes }
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();

    for ch in text.chars() {
        if ch.is_alphanumeric() || ch.is_numeric() {
            current_token.push(ch);
        } else {
            if !current_token.is_empty() {
                tokens.push(current_token.clone());
                current_token.clear();
            }
        }
    }

    if !current_token.is_empty() {
        tokens.push(current_token);
    }

    tokens
}

fn normalize_token(token: &str) -> String {
    let en_stemmer = Stemmer::create(rust_stemmers::Algorithm::English);

    en_stemmer.stem(token.to_lowercase().as_str()).to_string()
}

fn is_stopword(word: &str) -> bool {
    let empty: &[&str] = &[];
    stopwords::Spark::stopwords(stopwords::Language::English)
        .unwrap_or(empty)
        .contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tsvector() {
        let result = to_tsvector_weighted("The quick brown fox jumps over the lazy dog", 'D');

        let terms: Vec<&String> = result.lexemes.iter().map(|l| &l.term).collect();
        assert!(terms.contains(&&"quick".to_string()));
        assert!(terms.contains(&&"brown".to_string()));
        assert!(terms.contains(&&"fox".to_string()));
        assert!(!terms.contains(&&"the".to_string()));

        let fox_lexeme = result.lexemes.iter().find(|l| l.term == "fox").unwrap();
        assert_eq!(fox_lexeme.positions, vec![4]);
        assert_eq!(fox_lexeme.weight, 'D');
    }

    #[test]
    fn test_duplicate_words() {
        let result = to_tsvector_weighted("cat dog cat bird dog", 'A');

        let cat_lexeme = result.lexemes.iter().find(|l| l.term == "cat").unwrap();
        assert_eq!(cat_lexeme.positions, vec![1, 3]);

        let dog_lexeme = result.lexemes.iter().find(|l| l.term == "dog").unwrap();
        assert_eq!(dog_lexeme.positions, vec![2, 5]);
    }

    #[test]
    fn test_stemming() {
        let result = to_tsvector_weighted("running cats jumped", 'B');

        let terms: Vec<&String> = result.lexemes.iter().map(|l| &l.term).collect();
        assert!(terms.contains(&&"run".to_string())); // "running" -> "run"
        assert!(terms.contains(&&"cat".to_string())); // "cats" -> "cat"
        assert!(terms.contains(&&"jump".to_string())); // "jumped" -> "jump"
    }

    #[test]
    fn test_display() {
        let result = to_tsvector_weighted("hello world hello", 'C');
        let display = format!("{}", result);

        // Should be sorted alphabetically and show positions
        assert!(display.contains("'hello':1,3"));
        assert!(display.contains("'world':2"));
    }
}
