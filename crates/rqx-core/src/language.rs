use std::path::Path;

use serde::{Deserialize, Serialize};

/// The languages Rqx can analyze. A finding carries one so that a rule which cannot
/// exist in a language is a declared exemption rather than a silent absence.
#[derive(
    Copy,
    Clone,
    Debug,
    Deserialize,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// Spelled out, because kebab-case would make it `type-script` in `rqx.yaml`
    /// while `rqx rules --language` takes the slug — one name, two spellings.
    #[serde(rename = "typescript")]
    TypeScript,
    Python,
    Cpp,
    Java,
    Go,
}

impl Language {
    pub const ALL: [Self; 5] = [
        Self::TypeScript,
        Self::Python,
        Self::Cpp,
        Self::Java,
        Self::Go,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Go => "go",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Cpp => "C++",
            Self::Java => "Java",
            Self::Go => "Go",
        }
    }

    /// `.h` is claimed by C++ because Rqx has no C backend to hand it to, and the C++
    /// grammar parses C headers correctly.
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::TypeScript => &["ts", "tsx", "mts", "cts"],
            Self::Python => &["py", "pyi"],
            Self::Cpp => &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h", "ipp"],
            Self::Java => &["java"],
            Self::Go => &["go"],
        }
    }

    /// Declarations live in these; a rule that judges a whole file, such as
    /// `unused-file`, has to know that deleting a header is a different act from
    /// deleting the translation unit that includes it.
    pub const fn is_header(self, extension: &str) -> bool {
        matches!(self, Self::Cpp)
            && matches!(
                extension.as_bytes(),
                b"hpp" | b"hh" | b"hxx" | b"h" | b"ipp"
            )
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.slug() == slug)
    }

    pub fn of_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::ALL
            .into_iter()
            .find(|language| language.extensions().contains(&extension))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_extension_belongs_to_exactly_one_language() {
        for language in Language::ALL {
            for extension in language.extensions() {
                let owners: Vec<Language> = Language::ALL
                    .into_iter()
                    .filter(|other| other.extensions().contains(extension))
                    .collect();
                assert_eq!(owners, vec![language], "`{extension}` is claimed twice");
            }
        }
    }

    #[test]
    fn a_path_resolves_to_its_language() {
        assert_eq!(
            Language::of_path(Path::new("a/b.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::of_path(Path::new("a/b.py")),
            Some(Language::Python)
        );
        assert_eq!(Language::of_path(Path::new("a/b.hpp")), Some(Language::Cpp));
        assert_eq!(
            Language::of_path(Path::new("a/B.java")),
            Some(Language::Java)
        );
        assert_eq!(Language::of_path(Path::new("a/b.go")), Some(Language::Go));
        assert_eq!(Language::of_path(Path::new("a/b.md")), None);
        assert_eq!(Language::of_path(Path::new("Makefile")), None);
    }

    #[test]
    fn a_slug_is_also_how_the_config_spells_it() {
        for language in Language::ALL {
            let yaml = format!("value: {}\n", language.slug());
            #[derive(serde::Deserialize)]
            struct Holder {
                value: Language,
            }
            let held: Holder = serde_saphyr::from_str(&yaml)
                .unwrap_or_else(|error| panic!("{}: {error}", language.slug()));
            assert_eq!(held.value, language);
        }
    }

    #[test]
    fn every_slug_round_trips() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.slug()), Some(language));
        }
        assert_eq!(Language::parse("rust"), None);
    }
}
