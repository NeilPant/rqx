//! Who owns a file, read from a CODEOWNERS file.
//!
//! GitHub and GitLab share the pattern language, which follows `.gitignore`, and differ
//! in one thing: GitLab groups rules into `[Sections]`. Within one section the last
//! rule that matches a path decides its owners; a path takes the owners of every
//! section that has a matching rule. A file without sections is one section.

/// Which forge's reading of the file to apply.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialect {
    GitHub,
    GitLab,
}

impl Dialect {
    /// A file under `.github/` or `.gitlab/` says which forge it is for. One at the
    /// root or in `docs/` is read as GitLab only when it uses a section header, since
    /// GitHub has no such line.
    pub fn detect(path: &str, text: &str) -> Self {
        let inside = |folder: &str| path.split('/').any(|part| part == folder);
        let sectioned = || {
            text.lines()
                .any(|line| Header::parse(line.trim()).is_some())
        };
        if !inside(".github") && (inside(".gitlab") || sectioned()) {
            Self::GitLab
        } else {
            Self::GitHub
        }
    }
}

/// The rules of one CODEOWNERS file, ready to answer who owns a path.
#[derive(Clone, Debug, Default)]
pub struct CodeOwners {
    sections: Vec<Section>,
}

#[derive(Clone, Debug, Default)]
struct Section {
    name: String,
    rules: Vec<Rule>,
}

#[derive(Clone, Debug)]
struct Rule {
    pattern: Pattern,
    owners: Vec<String>,
}

impl CodeOwners {
    pub fn parse(text: &str, dialect: Dialect) -> Self {
        let mut sections = vec![Section::default()];
        let mut current = 0;
        let mut defaults: Vec<String> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if dialect == Dialect::GitLab
                && let Some(header) = Header::parse(line)
            {
                current = section_named(&mut sections, &header.name);
                defaults = header.owners;
                continue;
            }
            let words = words(line);
            let Some((pattern, owners)) = words.split_first() else {
                continue;
            };
            // GitHub does not support negation, so a rule written that way is not a rule.
            if pattern.starts_with('!') {
                continue;
            }
            let owners = if owners.is_empty() {
                defaults.clone()
            } else {
                owners.iter().map(|owner| (*owner).to_owned()).collect()
            };
            sections[current].rules.push(Rule {
                pattern: Pattern::parse(pattern),
                owners,
            });
        }
        Self { sections }
    }

    /// Whether the file holds any rule at all.
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(|section| section.rules.is_empty())
    }

    /// The owners of `path`, relative to the repository root with `/` separators, in
    /// the order the file lists them. Empty when no rule matches or the last one that
    /// does names nobody.
    pub fn owners_of(&self, path: &str) -> Vec<&str> {
        let components: Vec<&str> = path
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect();
        let mut owners: Vec<&str> = Vec::new();
        for section in &self.sections {
            let decided = section
                .rules
                .iter()
                .rev()
                .find(|rule| rule.pattern.matches(&components));
            for owner in decided.into_iter().flat_map(|rule| &rule.owners) {
                if !owners.contains(&owner.as_str()) {
                    owners.push(owner);
                }
            }
        }
        owners
    }
}

/// Section names are not case sensitive, and two headers with one name are one section.
fn section_named(sections: &mut Vec<Section>, name: &str) -> usize {
    let known = sections
        .iter()
        .position(|section| section.name.eq_ignore_ascii_case(name));
    known.unwrap_or_else(|| {
        sections.push(Section {
            name: name.to_owned(),
            rules: Vec::new(),
        });
        sections.len() - 1
    })
}

/// `[Name]`, `^[Name]` (optional approval) or `[Name][2]` (approvals required), then the
/// owners that every rule of the section without its own falls back to. Whether a
/// section is optional or needs approvals changes who must approve, not who owns.
struct Header {
    name: String,
    owners: Vec<String>,
}

impl Header {
    fn parse(line: &str) -> Option<Self> {
        let rest = line.strip_prefix('^').unwrap_or(line).strip_prefix('[')?;
        let (name, mut rest) = rest.split_once(']')?;
        if name.trim().is_empty() {
            return None;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some((approvals, after)) = after.split_once(']')
            && !approvals.is_empty()
            && approvals.bytes().all(|byte| byte.is_ascii_digit())
        {
            rest = after;
        }
        if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
            return None;
        }
        Some(Self {
            name: name.trim().to_owned(),
            owners: words(rest.trim()).into_iter().map(str::to_owned).collect(),
        })
    }
}

/// The words of a line, split on whitespace a backslash has not escaped, up to a `#`
/// that starts a word. Escapes stay in the text: the pattern reads them itself.
fn words(line: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    let mut escaped = false;
    for (at, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (character, start) {
            ('\\', None) => {
                start = Some(at);
                escaped = true;
            }
            ('\\', Some(_)) => escaped = true,
            (space, Some(from)) if space.is_whitespace() => {
                words.push(&line[from..at]);
                start = None;
            }
            (space, None) if space.is_whitespace() => {}
            ('#', None) => return words,
            (_, None) => start = Some(at),
            (_, Some(_)) => {}
        }
    }
    if let Some(from) = start {
        words.push(&line[from..]);
    }
    words
}

/// A path pattern, compiled once. `*` and `?` stay within one path component, a
/// component of `**` stands for any number of directories, and `[` and `]` are
/// ordinary characters, since CODEOWNERS has no character classes.
#[derive(Clone, Debug)]
struct Pattern {
    /// A pattern with a slash anywhere but the end names a path from the root;
    /// without one it names a file or directory at any depth.
    anchored: bool,
    reach: Reach,
    segments: Vec<Segment>,
}

/// What the end of a pattern may land on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Reach {
    /// A name with a wildcard in it: only the path it matches. GitHub documents
    /// `docs/*` as owning `docs/guide.md` and not `docs/guide/intro.md`.
    Exactly,
    /// A plain name: the path, or a directory above it, which takes its contents.
    Inside,
    /// A trailing slash: a directory, so only what is inside one and never a file of
    /// that name.
    BelowDirectory,
}

#[derive(Clone, Debug)]
enum Segment {
    AnyDirectories,
    Name(Vec<Piece>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Literal(char),
    AnyCharacter,
    AnyRun,
}

impl Pattern {
    fn parse(text: &str) -> Self {
        let directory_only = text.ends_with('/');
        let trimmed = text.trim_end_matches('/');
        let anchored = trimmed.contains('/');
        let mut segments: Vec<Segment> = Vec::new();
        for part in trimmed.split('/').filter(|part| !part.is_empty()) {
            let segment = if part == "**" {
                Segment::AnyDirectories
            } else {
                Segment::Name(pieces(part))
            };
            let repeated = matches!(
                (segments.last(), &segment),
                (Some(Segment::AnyDirectories), Segment::AnyDirectories)
            );
            if !repeated {
                segments.push(segment);
            }
        }
        let reach = match segments.last() {
            _ if directory_only => Reach::BelowDirectory,
            Some(Segment::Name(pieces))
                if pieces
                    .iter()
                    .any(|piece| !matches!(piece, Piece::Literal(_))) =>
            {
                Reach::Exactly
            }
            _ => Reach::Inside,
        };
        Self {
            anchored,
            reach,
            segments,
        }
    }

    /// Whether the pattern names the path or a directory above it.
    fn matches(&self, components: &[&str]) -> bool {
        if self.anchored {
            return fits(&self.segments, components, self.reach);
        }
        (0..components.len()).any(|from| fits(&self.segments, &components[from..], self.reach))
    }
}

/// Whether the segments match the start of `components`, with what is left over
/// allowed or required by `reach`.
fn fits(segments: &[Segment], components: &[&str], reach: Reach) -> bool {
    let Some((first, rest)) = segments.split_first() else {
        return match reach {
            Reach::Exactly => components.is_empty(),
            Reach::Inside => true,
            Reach::BelowDirectory => !components.is_empty(),
        };
    };
    match first {
        Segment::AnyDirectories if rest.is_empty() => !components.is_empty(),
        Segment::AnyDirectories => {
            (0..=components.len()).any(|skipped| fits(rest, &components[skipped..], reach))
        }
        Segment::Name(pieces) => components
            .split_first()
            .is_some_and(|(name, tail)| agrees(pieces, name) && fits(rest, tail, reach)),
    }
}

fn pieces(segment: &str) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut characters = segment.chars();
    while let Some(character) = characters.next() {
        let piece = match character {
            '\\' => Piece::Literal(characters.next().unwrap_or('\\')),
            '*' => Piece::AnyRun,
            '?' => Piece::AnyCharacter,
            other => Piece::Literal(other),
        };
        if piece == Piece::AnyRun && pieces.last() == Some(&Piece::AnyRun) {
            continue;
        }
        pieces.push(piece);
    }
    pieces
}

/// One component against a glob, remembering where the last `*` was so a mismatch can
/// stretch it by one character and try again.
fn agrees(pieces: &[Piece], name: &str) -> bool {
    let name: Vec<char> = name.chars().collect();
    let (mut at, mut on) = (0, 0);
    let mut stretch: Option<(usize, usize)> = None;
    while on < name.len() {
        match pieces.get(at) {
            Some(Piece::AnyRun) => {
                stretch = Some((at, on));
                at += 1;
            }
            Some(Piece::AnyCharacter) => {
                at += 1;
                on += 1;
            }
            Some(Piece::Literal(wanted)) if *wanted == name[on] => {
                at += 1;
                on += 1;
            }
            _ => match stretch {
                Some((star, from)) => {
                    stretch = Some((star, from + 1));
                    at = star + 1;
                    on = from + 1;
                }
                None => return false,
            },
        }
    }
    pieces[at..].iter().all(|piece| *piece == Piece::AnyRun)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(text: &str) -> CodeOwners {
        CodeOwners::parse(text, Dialect::GitHub)
    }

    fn gitlab(text: &str) -> CodeOwners {
        CodeOwners::parse(text, Dialect::GitLab)
    }

    #[test]
    fn the_last_matching_rule_decides() {
        let owners = github("* @everyone\n*.ts @frontend\nsrc/api/* @backend\n");
        assert_eq!(owners.owners_of("README.md"), ["@everyone"]);
        assert_eq!(owners.owners_of("web/app.ts"), ["@frontend"]);
        assert_eq!(owners.owners_of("src/api/app.ts"), ["@backend"]);
    }

    #[test]
    fn a_rule_without_owners_clears_what_an_earlier_one_set() {
        let owners = github("/apps/ @octocat\n/apps/github\n");
        assert_eq!(owners.owners_of("apps/web/main.ts"), ["@octocat"]);
        assert!(owners.owners_of("apps/github/main.ts").is_empty());
    }

    #[test]
    fn a_path_nothing_matches_has_no_owners() {
        let owners = github("/docs/ @docs\n");
        assert!(owners.owners_of("src/a.ts").is_empty());
        assert!(github("").owners_of("src/a.ts").is_empty());
    }

    #[test]
    fn a_name_without_a_slash_matches_at_any_depth() {
        let owners = github("*.js @js\nLICENSE @legal\n");
        assert_eq!(owners.owners_of("a.js"), ["@js"]);
        assert_eq!(owners.owners_of("deep/er/a.js"), ["@js"]);
        assert_eq!(owners.owners_of("sub/LICENSE"), ["@legal"]);
        assert!(owners.owners_of("a.jsx").is_empty());
    }

    #[test]
    fn a_leading_slash_anchors_to_the_root() {
        let owners = github("/build/logs @ops\n/README.md @docs\n");
        assert_eq!(owners.owners_of("build/logs/today.txt"), ["@ops"]);
        assert!(owners.owners_of("src/build/logs/today.txt").is_empty());
        assert_eq!(owners.owners_of("README.md"), ["@docs"]);
        assert!(owners.owners_of("docs/README.md").is_empty());
    }

    #[test]
    fn a_slash_in_the_middle_anchors_too() {
        let owners = github("docs/*.md @docs\n");
        assert_eq!(owners.owners_of("docs/a.md"), ["@docs"]);
        assert!(owners.owners_of("src/docs/a.md").is_empty());
    }

    #[test]
    fn one_star_stays_in_its_directory() {
        let owners = github("docs/* @docs\n");
        assert_eq!(owners.owners_of("docs/getting-started.md"), ["@docs"]);
        assert!(
            owners
                .owners_of("docs/build-app/troubleshooting.md")
                .is_empty()
        );
    }

    #[test]
    fn two_stars_cross_directories() {
        let owners = github("docs/** @docs\n**/logs @ops\na/**/b @mid\n");
        assert_eq!(owners.owners_of("docs/a/b/c.md"), ["@docs"]);
        assert_eq!(owners.owners_of("logs/x.txt"), ["@ops"]);
        assert_eq!(owners.owners_of("build/deep/logs/x.txt"), ["@ops"]);
        assert_eq!(owners.owners_of("a/b/x.ts"), ["@mid"]);
        assert_eq!(owners.owners_of("a/x/y/b/x.ts"), ["@mid"]);
    }

    #[test]
    fn a_trailing_double_star_needs_something_inside() {
        let owners = github("docs/** @docs\n");
        assert!(owners.owners_of("docs").is_empty());
    }

    #[test]
    fn a_directory_pattern_matches_the_directory_anywhere_and_never_a_file() {
        let owners = github("apps/ @apps\n");
        assert_eq!(owners.owners_of("apps/web/main.ts"), ["@apps"]);
        assert_eq!(owners.owners_of("packages/apps/main.ts"), ["@apps"]);
        assert!(owners.owners_of("packages/apps").is_empty());
    }

    #[test]
    fn an_anchored_directory_pattern_matches_only_at_the_root() {
        let owners = github("/apps/ @apps\n");
        assert_eq!(owners.owners_of("apps/web/main.ts"), ["@apps"]);
        assert!(owners.owners_of("packages/apps/main.ts").is_empty());
    }

    #[test]
    fn a_plain_name_matches_a_directory_and_what_is_inside_it() {
        let owners = github("scripts @tools\n");
        assert_eq!(owners.owners_of("scripts/build.sh"), ["@tools"]);
        assert_eq!(owners.owners_of("a/scripts"), ["@tools"]);
    }

    #[test]
    fn a_lone_star_owns_everything() {
        let owners = github("* @all\n");
        assert_eq!(owners.owners_of("a.ts"), ["@all"]);
        assert_eq!(owners.owners_of("a/b/c.ts"), ["@all"]);
    }

    #[test]
    fn a_question_mark_is_one_character() {
        let owners = github("v?.ts @versions\n");
        assert_eq!(owners.owners_of("v1.ts"), ["@versions"]);
        assert!(owners.owners_of("v10.ts").is_empty());
        assert!(owners.owners_of("v.ts").is_empty());
    }

    #[test]
    fn a_star_inside_a_name_matches_a_run_of_characters() {
        let owners = github("test_*_spec.ts @qa\n");
        assert_eq!(owners.owners_of("src/test_login_spec.ts"), ["@qa"]);
        assert_eq!(owners.owners_of("src/test__spec.ts"), ["@qa"]);
        assert!(owners.owners_of("src/test_login_spec.tsx").is_empty());
    }

    #[test]
    fn brackets_are_ordinary_characters() {
        let owners = github("[ab].ts @brackets\n");
        assert_eq!(owners.owners_of("[ab].ts"), ["@brackets"]);
        assert!(owners.owners_of("a.ts").is_empty());
    }

    #[test]
    fn negation_is_not_supported_so_the_rule_is_ignored() {
        let owners = github("* @all\n!*.md @nobody\n");
        assert_eq!(owners.owners_of("README.md"), ["@all"]);
    }

    #[test]
    fn comments_are_skipped_whole_or_trailing() {
        let owners = github("# a note\n\n*.ts @ts # the typescript team\n  # indented note\n");
        assert_eq!(owners.owners_of("a.ts"), ["@ts"]);
        assert!(owners.owners_of("a.md").is_empty());
    }

    #[test]
    fn an_escaped_hash_is_part_of_the_pattern() {
        let owners = github("\\#notes.md @writers\n");
        assert_eq!(owners.owners_of("#notes.md"), ["@writers"]);
    }

    #[test]
    fn an_escaped_space_is_part_of_the_pattern() {
        let owners = github("/my\\ docs/ @docs\n");
        assert_eq!(owners.owners_of("my docs/a.md"), ["@docs"]);
        assert!(owners.owners_of("my/docs/a.md").is_empty());
    }

    #[test]
    fn several_owners_keep_their_order_and_never_repeat() {
        let owners = github("*.ts @a b@example.com @org/team @a\n");
        assert_eq!(
            owners.owners_of("x.ts"),
            ["@a", "b@example.com", "@org/team"]
        );
    }

    #[test]
    fn windows_line_endings_and_tabs_read_the_same() {
        let owners = github("*.ts\t@ts\r\n/docs/  @docs\r\n");
        assert_eq!(owners.owners_of("a.ts"), ["@ts"]);
        assert_eq!(owners.owners_of("docs/a.md"), ["@docs"]);
    }

    #[test]
    fn a_leading_dot_slash_on_the_path_is_ignored() {
        let owners = github("/src/ @src\n");
        assert_eq!(owners.owners_of("./src/a.ts"), ["@src"]);
    }

    #[test]
    fn matching_is_case_sensitive() {
        let owners = github("README.md @docs\n");
        assert!(owners.owners_of("readme.md").is_empty());
    }

    #[test]
    fn github_reads_a_bracketed_line_as_a_pattern() {
        let owners = github("[Docs] @docs\n* @all\n");
        assert_eq!(owners.owners_of("a.ts"), ["@all"]);
    }

    #[test]
    fn a_section_decides_for_itself_and_the_owners_add_up() {
        let owners = gitlab(
            "\
* @everyone
[Docs]
*.md @docs
[Security]
src/auth/ @security
*.md @writers
",
        );
        assert_eq!(
            owners.owners_of("src/auth/a.ts"),
            ["@everyone", "@security"]
        );
        assert_eq!(
            owners.owners_of("README.md"),
            ["@everyone", "@docs", "@writers"]
        );
    }

    #[test]
    fn the_last_rule_wins_inside_a_section() {
        let owners = gitlab("[Core]\n*.ts @a\nsrc/*.ts @b\n");
        assert_eq!(owners.owners_of("src/x.ts"), ["@b"]);
    }

    #[test]
    fn a_section_owner_is_the_default_for_its_rules_without_one() {
        let owners = gitlab("[Docs] @docs @writers\n*.md\n/legal/ @lawyers\n");
        assert_eq!(owners.owners_of("a.md"), ["@docs", "@writers"]);
        assert_eq!(owners.owners_of("legal/a.txt"), ["@lawyers"]);
    }

    #[test]
    fn an_optional_section_and_its_approval_count_still_name_owners() {
        let owners = gitlab("^[Optional] @maybe\n*.md\n[Approved][2] @must\n*.ts\n");
        assert_eq!(owners.owners_of("a.md"), ["@maybe"]);
        assert_eq!(owners.owners_of("a.ts"), ["@must"]);
    }

    #[test]
    fn a_section_name_is_not_case_sensitive_and_repeats_are_one_section() {
        let owners = gitlab("[Docs]\n*.md @first\n[Other]\n*.ts @ts\n[docs]\n*.md @second\n");
        assert_eq!(owners.owners_of("a.md"), ["@second"]);
        assert_eq!(owners.owners_of("a.ts"), ["@ts"]);
    }

    #[test]
    fn a_section_header_may_carry_a_comment() {
        let owners = gitlab("[Docs] @docs # who reviews prose\n*.md\n");
        assert_eq!(owners.owners_of("a.md"), ["@docs"]);
    }

    #[test]
    fn a_file_without_rules_is_empty() {
        assert!(github("# nothing here\n").is_empty());
        assert!(!github("* @all\n").is_empty());
        assert!(gitlab("[Docs]\n").is_empty());
    }

    #[test]
    fn the_dialect_follows_the_folder_and_then_the_text() {
        assert_eq!(
            Dialect::detect(".github/CODEOWNERS", "[A]\n"),
            Dialect::GitHub
        );
        assert_eq!(
            Dialect::detect(".gitlab/CODEOWNERS", "* @a\n"),
            Dialect::GitLab
        );
        assert_eq!(Dialect::detect("CODEOWNERS", "* @a\n"), Dialect::GitHub);
        assert_eq!(
            Dialect::detect("docs/CODEOWNERS", "^[Docs]\n* @a\n"),
            Dialect::GitLab
        );
    }
}
