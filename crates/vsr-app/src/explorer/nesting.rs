//! File nesting (`explorer.fileNesting.*`, ADR-0052): related files in a folder are shown under one
//! parent file. A parent pattern (`*.ts`, `package.json`) names the parent; its child patterns
//! (`${capture}.js`, `package-lock.json`) name the files nested under it. This is the matcher
//! behind the tree: `ExplorerFileNestingTrie::nest` maps each root file to the files nested in it.
//!
//! upstream: src/vs/workbench/contrib/files/common/explorerFileNestingTrie.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/test/browser/explorerFileNestingTrie.test.ts @ 50e1350fc78d (the `is fast` test is skipped upstream too)

use std::collections::{HashMap, HashSet};

/// The parts of a file name a child pattern can refer to: `${basename}`, `${extname}`, `${dirname}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attributes<'a> {
    pub dirname: &'a str,
    pub basename: &'a str,
    pub extname: &'a str,
}

/// One piece of a child pattern: literal text, or a substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    Capture,
    Basename,
    Dirname,
    Extname,
}

/// A child pattern with its substitutions (`SubstitutionString` upstream). `${x}` and `$(x)` are both
/// accepted for `x` in capture, basename, dirname and extname; anything else stays literal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Substitution {
    tokens: Vec<Token>,
}

impl Substitution {
    fn new(pattern: &str) -> Self {
        let mut tokens = Vec::new();
        let mut rest = pattern;
        while let Some(pos) = rest.find('$') {
            let after = &rest[pos + 1..];
            match parse_token(after) {
                Some((token, len)) => {
                    tokens.push(Token::Text(rest[..pos].to_string()));
                    tokens.push(token);
                    rest = &after[len..];
                }
                None => {
                    tokens.push(Token::Text(rest[..pos + 1].to_string()));
                    rest = after;
                }
            }
        }
        tokens.push(Token::Text(rest.to_string()));
        Self { tokens }
    }

    fn substitute(&self, attrs: &Attributes, capture: Option<&str>) -> String {
        self.tokens
            .iter()
            .map(|t| match t {
                Token::Text(s) => s.as_str(),
                Token::Capture => capture.unwrap_or(""),
                Token::Basename => attrs.basename,
                Token::Dirname => attrs.dirname,
                Token::Extname => attrs.extname,
            })
            .collect()
    }
}

/// The substitution at the start of `after` (the text following a `$`), with its length in bytes.
fn parse_token(after: &str) -> Option<(Token, usize)> {
    let close = match after.as_bytes().first()? {
        b'(' => ')',
        b'{' => '}',
        _ => return None,
    };
    let inner = &after[1..];
    for (name, token) in [
        ("capture", Token::Capture),
        ("basename", Token::Basename),
        ("dirname", Token::Dirname),
        ("extname", Token::Extname),
    ] {
        if inner
            .strip_prefix(name)
            .is_some_and(|tail| tail.starts_with(close))
        {
            return Some((token, 1 + name.len() + 1));
        }
    }
    None
}

/// A trie over the characters of a suffix pattern, read from its end (`SufTrie` upstream). A key
/// `*.npmrc` is stored under `c`, `r`, `m`, `p`, `n`, `.`, and the star at the front matches any
/// prefix. A key `""` (an exact name) is stored under `epsilon`.
#[derive(Debug, Default)]
pub struct SufTrie {
    star: Vec<Substitution>,
    epsilon: Vec<Substitution>,
    map: HashMap<char, SufTrie>,
}

impl SufTrie {
    pub fn add(&mut self, key: &str, value: &str) {
        if key == "*" {
            self.star.push(Substitution::new(value));
        } else if key.is_empty() {
            self.epsilon.push(Substitution::new(value));
        } else {
            let tail = key
                .chars()
                .next_back()
                .expect("a non-empty key has a last character");
            let rest = &key[..key.len() - tail.len_utf8()];
            // A key ending in `*` is malformed (upstream throws). It is ignored, so a bad user
            // pattern cannot break the tree.
            if tail != '*' {
                self.map.entry(tail).or_default().add(rest, value);
            }
        }
    }

    pub fn get(&self, key: &str, attrs: &Attributes) -> Vec<String> {
        let mut results = Vec::new();
        if key.is_empty() {
            results.extend(self.epsilon.iter().map(|s| s.substitute(attrs, None)));
        }
        if !self.star.is_empty() {
            results.extend(self.star.iter().map(|s| s.substitute(attrs, Some(key))));
        }
        if let Some(existing) = key
            .chars()
            .next_back()
            .and_then(|tail| self.map.get(&tail).map(|e| (e, tail)))
        {
            results.extend(
                existing
                    .0
                    .get(&key[..key.len() - existing.1.len_utf8()], attrs),
            );
        }
        results
    }
}

/// A trie over the characters of a prefix pattern (`PreTrie` upstream). Keys starting with `*` go to
/// the suffix trie under the root; the other characters are walked from the front.
#[derive(Debug, Default)]
pub struct PreTrie {
    value: SufTrie,
    map: HashMap<char, PreTrie>,
}

impl PreTrie {
    pub fn add(&mut self, key: &str, value: &str) {
        match key.chars().next() {
            None | Some('*') => self.value.add(key, value),
            Some(head) => self
                .map
                .entry(head)
                .or_default()
                .add(&key[head.len_utf8()..], value),
        }
    }

    pub fn get(&self, key: &str, attrs: &Attributes) -> Vec<String> {
        let mut results = self.value.get(key, attrs);
        if let Some(existing) = key
            .chars()
            .next()
            .and_then(|head| self.map.get(&head).map(|e| (e, head)))
        {
            results.extend(existing.0.get(&key[existing.1.len_utf8()..], attrs));
        }
        results
    }
}

/// The nesting rules of the Explorer: a parent pattern and the child patterns it nests.
#[derive(Debug, Default)]
pub struct ExplorerFileNestingTrie {
    root: PreTrie,
}

/// A parent and the names nested under it, in the order they were found.
pub type Nests = Vec<(String, Vec<String>)>;

/// The default `explorer.fileNesting.patterns` (upstream `files.contribution.ts`).
pub fn default_rules() -> Vec<(String, Vec<String>)> {
    [
        ("*.ts", "${capture}.js"),
        (
            "*.js",
            "${capture}.js.map, ${capture}.min.js, ${capture}.d.ts",
        ),
        ("*.jsx", "${capture}.js"),
        ("*.tsx", "${capture}.ts"),
        ("tsconfig.json", "tsconfig.*.json"),
        (
            "package.json",
            "package-lock.json, yarn.lock, pnpm-lock.yaml, bun.lockb, bun.lock",
        ),
    ]
    .iter()
    .map(|(parent, children)| {
        let children = children.split(',').map(|c| c.trim().to_string()).collect();
        (parent.to_string(), children)
    })
    .collect()
}

/// `explorer.fileNesting.patterns` as the rules: each item is a parent pattern (trimmed) and its
/// comma-separated child patterns (trimmed, empty ones dropped). Items that are not strings, or have
/// an empty side, are skipped (upstream `explorerModel.ts`, `fileNester`).
pub fn rules_from_value(value: Option<&serde_json::Value>) -> Vec<(String, Vec<String>)> {
    let Some(serde_json::Value::Object(map)) = value else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(parent, children)| {
            let serde_json::Value::String(children) = children else {
                return None;
            };
            let parent = parent.trim();
            if parent.is_empty() || children.is_empty() {
                return None;
            }
            let children: Vec<String> = children
                .split(',')
                .map(|c| c.replace('\u{200b}', "").trim().to_string())
                .filter(|c| !c.is_empty())
                .collect();
            Some((parent.to_string(), children))
        })
        .collect()
}

fn attributes<'a>(filename: &'a str, dirname: &'a str) -> Attributes<'a> {
    match filename.rfind('.') {
        Some(dot) if dot >= 1 => Attributes {
            dirname,
            basename: &filename[..dot],
            extname: &filename[dot + 1..],
        },
        _ => Attributes {
            dirname,
            basename: filename,
            extname: "",
        },
    }
}

/// `values` without repeats, keeping the first of each (a JavaScript `Set` in insertion order).
fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

impl ExplorerFileNestingTrie {
    pub fn new(config: &[(String, Vec<String>)]) -> Self {
        let mut root = PreTrie::default();
        for (parent, children) in config {
            for child in children {
                root.add(parent, child);
            }
        }
        Self { root }
    }

    /// The files of `files` (names in the folder `dirname`) that are nested, grouped under the root
    /// files they nest in. A file that is a root is a key even when nothing nests in it; a file that
    /// nests under another is listed only as a child. A file may match its own child patterns, but
    /// it is never its own parent.
    pub fn nest(&self, files: &[String], dirname: &str) -> Nests {
        let mut parent_finder = PreTrie::default();
        for potential_parent in files {
            let attrs = attributes(potential_parent, dirname);
            for child in self.root.get(potential_parent, &attrs) {
                parent_finder.add(&child, potential_parent);
            }
        }

        let find_all_root_ancestors = |file: &str, seen: &mut HashSet<String>| -> Vec<String> {
            fn walk(
                file: &str,
                dirname: &str,
                finder: &PreTrie,
                seen: &mut HashSet<String>,
            ) -> Vec<String> {
                if !seen.insert(file.to_string()) {
                    return Vec::new();
                }
                let attrs = attributes(file, dirname);
                let mut ancestors: Vec<String> = unique(finder.get(file, &attrs));
                ancestors.retain(|a| a != file);
                if ancestors.is_empty() {
                    return vec![file.to_string()];
                }
                ancestors
                    .iter()
                    .flat_map(|a| walk(a, dirname, finder, seen))
                    .collect()
            }
            walk(file, dirname, &parent_finder, seen)
        };

        let mut index: HashMap<String, usize> = HashMap::new();
        let mut result: Nests = Vec::new();
        for file in files {
            let mut ancestors = find_all_root_ancestors(file, &mut HashSet::new());
            if ancestors.is_empty() {
                ancestors = vec![file.clone()];
            }
            for ancestor in ancestors {
                let slot = *index.entry(ancestor.clone()).or_insert_with(|| {
                    result.push((ancestor.clone(), Vec::new()));
                    result.len() - 1
                });
                if *file != ancestor && !result[slot].1.contains(file) {
                    result[slot].1.push(file.clone());
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake() -> Attributes<'static> {
        Attributes {
            dirname: "mydir",
            basename: "",
            extname: "",
        }
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// `assertMapEquals`: the same parents, each with the same children (order ignored).
    fn assert_nests(actual: &Nests, expected: &[(&str, &[&str])]) {
        assert_eq!(actual.len(), expected.len(), "parents: {actual:?}");
        for (parent, children) in expected {
            let (_, got) = actual
                .iter()
                .find(|(p, _)| p == parent)
                .unwrap_or_else(|| panic!("no parent {parent} in {actual:?}"));
            let got: HashSet<&str> = got.iter().map(|c| c.as_str()).collect();
            let want: HashSet<&str> = children.iter().copied().collect();
            assert_eq!(got, want, "children of {parent}");
            assert_eq!(got.len(), children.len(), "no duplicates for {parent}");
        }
    }

    // ---- SufTrie ----

    // upstream: SufTrie > exactMatches
    #[test]
    fn suf_exact_matches() {
        let mut t = SufTrie::default();
        t.add(".npmrc", "MyKey");
        assert_eq!(t.get(".npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), Vec::<String>::new());
    }

    // upstream: SufTrie > starMatches
    #[test]
    fn suf_star_matches() {
        let mut t = SufTrie::default();
        t.add("*.npmrc", "MyKey");
        assert_eq!(t.get(".npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["MyKey"]));
    }

    // upstream: SufTrie > starSubstitutes
    #[test]
    fn suf_star_substitutes() {
        let mut t = SufTrie::default();
        t.add("*.npmrc", "${capture}.json");
        assert_eq!(t.get(".npmrc", &fake()), s(&[".json"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), s(&["a.json"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["a.b.c.d.json"]));
    }

    // upstream: SufTrie > multiMatches
    #[test]
    fn suf_multi_matches() {
        let mut t = SufTrie::default();
        t.add("*.npmrc", "Key1");
        t.add("*.json", "Key2");
        t.add("*d.npmrc", "Key3");
        assert_eq!(t.get(".npmrc", &fake()), s(&["Key1"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".json", &fake()), s(&["Key2"]));
        assert_eq!(t.get("a.json", &fake()), s(&["Key2"]));
        assert_eq!(t.get("a.npmrc", &fake()), s(&["Key1"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["Key1", "Key3"]));
    }

    // upstream: SufTrie > multiSubstitutes
    #[test]
    fn suf_multi_substitutes() {
        let mut t = SufTrie::default();
        t.add("*.npmrc", "Key1.${capture}.js");
        t.add("*.json", "Key2.${capture}.js");
        t.add("*d.npmrc", "Key3.${capture}.js");
        assert_eq!(t.get(".npmrc", &fake()), s(&["Key1..js"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".json", &fake()), s(&["Key2..js"]));
        assert_eq!(t.get("a.json", &fake()), s(&["Key2.a.js"]));
        assert_eq!(t.get("a.npmrc", &fake()), s(&["Key1.a.js"]));
        assert_eq!(
            t.get("a.b.cd.npmrc", &fake()),
            s(&["Key1.a.b.cd.js", "Key3.a.b.c.js"])
        );
        assert_eq!(
            t.get("a.b.c.d.npmrc", &fake()),
            s(&["Key1.a.b.c.d.js", "Key3.a.b.c..js"])
        );
    }

    // ---- PreTrie ----

    // upstream: PreTrie > exactMatches
    #[test]
    fn pre_exact_matches() {
        let mut t = PreTrie::default();
        t.add(".npmrc", "MyKey");
        assert_eq!(t.get(".npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), Vec::<String>::new());
    }

    // upstream: PreTrie > starMatches
    #[test]
    fn pre_star_matches() {
        let mut t = PreTrie::default();
        t.add("*.npmrc", "MyKey");
        assert_eq!(t.get(".npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), s(&["MyKey"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["MyKey"]));
    }

    // upstream: PreTrie > starSubstitutes
    #[test]
    fn pre_star_substitutes() {
        let mut t = PreTrie::default();
        t.add("*.npmrc", "${capture}.json");
        assert_eq!(t.get(".npmrc", &fake()), s(&[".json"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get("a.npmrc", &fake()), s(&["a.json"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["a.b.c.d.json"]));
    }

    // upstream: PreTrie > multiMatches
    #[test]
    fn pre_multi_matches() {
        let mut t = PreTrie::default();
        t.add("*.npmrc", "Key1");
        t.add("*.json", "Key2");
        t.add("*d.npmrc", "Key3");
        assert_eq!(t.get(".npmrc", &fake()), s(&["Key1"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".json", &fake()), s(&["Key2"]));
        assert_eq!(t.get("a.json", &fake()), s(&["Key2"]));
        assert_eq!(t.get("a.npmrc", &fake()), s(&["Key1"]));
        assert_eq!(t.get("a.b.c.d.npmrc", &fake()), s(&["Key1", "Key3"]));
    }

    // upstream: PreTrie > multiSubstitutes
    #[test]
    fn pre_multi_substitutes() {
        let mut t = PreTrie::default();
        t.add("*.npmrc", "Key1.${capture}.js");
        t.add("*.json", "Key2.${capture}.js");
        t.add("*d.npmrc", "Key3.${capture}.js");
        assert_eq!(t.get(".npmrc", &fake()), s(&["Key1..js"]));
        assert_eq!(t.get("npmrc", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".npmrcs", &fake()), Vec::<String>::new());
        assert_eq!(t.get(".json", &fake()), s(&["Key2..js"]));
        assert_eq!(t.get("a.json", &fake()), s(&["Key2.a.js"]));
        assert_eq!(t.get("a.npmrc", &fake()), s(&["Key1.a.js"]));
        assert_eq!(
            t.get("a.b.cd.npmrc", &fake()),
            s(&["Key1.a.b.cd.js", "Key3.a.b.c.js"])
        );
        assert_eq!(
            t.get("a.b.c.d.npmrc", &fake()),
            s(&["Key1.a.b.c.d.js", "Key3.a.b.c..js"])
        );
    }

    // upstream: PreTrie > emptyMatches
    #[test]
    fn pre_empty_matches() {
        let mut t = PreTrie::default();
        t.add("package*json", "package");
        assert_eq!(t.get("package.json", &fake()), s(&["package"]));
        assert_eq!(t.get("packagejson", &fake()), s(&["package"]));
        assert_eq!(t.get("package-lock.json", &fake()), s(&["package"]));
    }

    // ---- StarTrie (ExplorerFileNestingTrie) ----

    // upstream: StarTrie > does added extension nesting
    #[test]
    fn does_added_extension_nesting() {
        let t = ExplorerFileNestingTrie::new(&[("*".into(), s(&["${capture}.*"]))]);
        let nesting = t.nest(
            &s(&[
                "file",
                "file.json",
                "boop.test",
                "boop.test1",
                "boop.test.1",
                "beep",
                "beep.test1",
                "beep.boop.test1",
                "beep.boop.test2",
                "beep.boop.a",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[
                ("file", &["file.json"]),
                ("boop.test", &["boop.test.1"]),
                ("boop.test1", &[]),
                (
                    "beep",
                    &[
                        "beep.test1",
                        "beep.boop.test1",
                        "beep.boop.test2",
                        "beep.boop.a",
                    ],
                ),
            ],
        );
    }

    // upstream: StarTrie > does ext specific nesting
    #[test]
    fn does_ext_specific_nesting() {
        let t = ExplorerFileNestingTrie::new(&[
            ("*.ts".into(), s(&["${capture}.js"])),
            ("*.js".into(), s(&["${capture}.map"])),
        ]);
        let nesting = t.nest(
            &s(&[
                "a.ts", "a.js", "a.jss", "ab.js", "b.js", "b.map", "c.ts", "c.js", "c.map", "d.ts",
                "d.map",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[
                ("a.ts", &["a.js"]),
                ("ab.js", &[]),
                ("a.jss", &[]),
                ("b.js", &["b.map"]),
                ("c.ts", &["c.js", "c.map"]),
                ("d.ts", &[]),
                ("d.map", &[]),
            ],
        );
    }

    // upstream: StarTrie > handles loops
    #[test]
    fn handles_loops() {
        let t = ExplorerFileNestingTrie::new(&[
            ("*.a".into(), s(&["${capture}.b", "${capture}.c"])),
            ("*.b".into(), s(&["${capture}.a"])),
            ("*.c".into(), s(&["${capture}.d"])),
            ("*.aa".into(), s(&["${capture}.bb"])),
            ("*.bb".into(), s(&["${capture}.cc", "${capture}.dd"])),
            ("*.cc".into(), s(&["${capture}.aa"])),
            ("*.dd".into(), s(&["${capture}.ee"])),
        ]);
        let nesting = t.nest(
            &s(&[
                ".a", ".b", ".c", ".d", "a.a", "a.b", "a.d", "a.aa", "a.bb", "a.cc", "b.aa",
                "b.bb", "c.bb", "c.cc", "d.aa", "d.cc", "e.aa", "e.bb", "e.dd", "e.ee", "f.aa",
                "f.bb", "f.cc", "f.dd", "f.ee",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[
                (".a", &[]),
                (".b", &[]),
                (".c", &[]),
                (".d", &[]),
                ("a.a", &[]),
                ("a.b", &[]),
                ("a.d", &[]),
                ("a.aa", &[]),
                ("a.bb", &[]),
                ("a.cc", &[]),
                ("b.aa", &["b.bb"]),
                ("c.bb", &["c.cc"]),
                ("d.cc", &["d.aa"]),
                ("e.aa", &["e.bb", "e.dd", "e.ee"]),
                ("f.aa", &[]),
                ("f.bb", &[]),
                ("f.cc", &[]),
                ("f.dd", &[]),
                ("f.ee", &[]),
            ],
        );
    }

    // upstream: StarTrie > does general bidirectional suffix matching
    #[test]
    fn does_general_bidirectional_suffix_matching() {
        let t = ExplorerFileNestingTrie::new(&[
            ("*-vsdoc.js".into(), s(&["${capture}.js"])),
            ("*.js".into(), s(&["${capture}-vscdoc.js"])),
        ]);
        let nesting = t.nest(&s(&["a-vsdoc.js", "a.js", "b.js", "b-vscdoc.js"]), "mydir");
        assert_nests(
            &nesting,
            &[("a-vsdoc.js", &["a.js"]), ("b.js", &["b-vscdoc.js"])],
        );
    }

    // upstream: StarTrie > does general bidirectional prefix matching
    #[test]
    fn does_general_bidirectional_prefix_matching() {
        let t = ExplorerFileNestingTrie::new(&[
            ("vsdoc-*.js".into(), s(&["${capture}.js"])),
            ("*.js".into(), s(&["vscdoc-${capture}.js"])),
        ]);
        let nesting = t.nest(&s(&["vsdoc-a.js", "a.js", "b.js", "vscdoc-b.js"]), "mydir");
        assert_nests(
            &nesting,
            &[("vsdoc-a.js", &["a.js"]), ("b.js", &["vscdoc-b.js"])],
        );
    }

    // upstream: StarTrie > does general bidirectional general matching
    #[test]
    fn does_general_bidirectional_general_matching() {
        let t = ExplorerFileNestingTrie::new(&[
            ("foo-*-bar.js".into(), s(&["${capture}.js"])),
            ("*.js".into(), s(&["bib-${capture}-bap.js"])),
        ]);
        let nesting = t.nest(
            &s(&["foo-a-bar.js", "a.js", "b.js", "bib-b-bap.js"]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[("foo-a-bar.js", &["a.js"]), ("b.js", &["bib-b-bap.js"])],
        );
    }

    // upstream: StarTrie > does extension specific path segment matching
    #[test]
    fn does_extension_specific_path_segment_matching() {
        let t = ExplorerFileNestingTrie::new(&[("*.js".into(), s(&["${capture}.*.js"]))]);
        let nesting = t.nest(
            &s(&["foo.js", "foo.test.js", "fooTest.js", "bar.js.js"]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[
                ("foo.js", &["foo.test.js"]),
                ("fooTest.js", &[]),
                ("bar.js.js", &[]),
            ],
        );
    }

    // upstream: StarTrie > does exact match nesting
    #[test]
    fn does_exact_match_nesting() {
        let t = ExplorerFileNestingTrie::new(&[
            (
                "package.json".into(),
                s(&[
                    ".npmrc",
                    "npm-shrinkwrap.json",
                    "yarn.lock",
                    ".yarnclean",
                    ".yarnignore",
                    ".yarn-integrity",
                    ".yarnrc",
                ]),
            ),
            ("bower.json".into(), s(&[".bowerrc"])),
        ]);
        let nesting = t.nest(
            &s(&[
                "package.json",
                ".npmrc",
                "npm-shrinkwrap.json",
                "yarn.lock",
                ".bowerrc",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting,
            &[
                (
                    "package.json",
                    &[".npmrc", "npm-shrinkwrap.json", "yarn.lock"],
                ),
                (".bowerrc", &[]),
            ],
        );
    }

    // upstream: StarTrie > eslint test
    #[test]
    fn eslint_test() {
        let t = ExplorerFileNestingTrie::new(&[(".eslintrc*".into(), s(&[".eslint*"]))]);
        let nesting1 = t.nest(&s(&[".eslintrc.json", ".eslintignore"]), "mydir");
        assert_nests(&nesting1, &[(".eslintrc.json", &[".eslintignore"])]);
        let nesting2 = t.nest(&s(&[".eslintrc", ".eslintignore"]), "mydir");
        assert_nests(&nesting2, &[(".eslintrc", &[".eslintignore"])]);
    }

    // upstream: StarTrie > basename expansion
    #[test]
    fn basename_expansion() {
        let t = ExplorerFileNestingTrie::new(&[("*-vsdoc.js".into(), s(&["${basename}.doc"]))]);
        let nesting1 = t.nest(
            &s(&["boop-vsdoc.js", "boop-vsdoc.doc", "boop.doc"]),
            "mydir",
        );
        assert_nests(
            &nesting1,
            &[("boop-vsdoc.js", &["boop-vsdoc.doc"]), ("boop.doc", &[])],
        );
    }

    // upstream: StarTrie > extname expansion
    #[test]
    fn extname_expansion() {
        let t = ExplorerFileNestingTrie::new(&[("*-vsdoc.js".into(), s(&["${extname}.doc"]))]);
        let nesting1 = t.nest(&s(&["boop-vsdoc.js", "js.doc", "boop.doc"]), "mydir");
        assert_nests(
            &nesting1,
            &[("boop-vsdoc.js", &["js.doc"]), ("boop.doc", &[])],
        );
    }

    // upstream: StarTrie > added segment matcher
    #[test]
    fn added_segment_matcher() {
        let t = ExplorerFileNestingTrie::new(&[("*".into(), s(&["${basename}.*.${extname}"]))]);
        let nesting1 = t.nest(
            &s(&[
                "some.file",
                "some.html.file",
                "some.html.nested.file",
                "other.file",
                "some.thing",
                "some.thing.else",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting1,
            &[
                ("some.file", &["some.html.file", "some.html.nested.file"]),
                ("other.file", &[]),
                ("some.thing", &[]),
                ("some.thing.else", &[]),
            ],
        );
    }

    // upstream: StarTrie > added segment matcher (old format)
    #[test]
    fn added_segment_matcher_old_format() {
        let t = ExplorerFileNestingTrie::new(&[("*".into(), s(&["$(basename).*.$(extname)"]))]);
        let nesting1 = t.nest(
            &s(&[
                "some.file",
                "some.html.file",
                "some.html.nested.file",
                "other.file",
                "some.thing",
                "some.thing.else",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting1,
            &[
                ("some.file", &["some.html.file", "some.html.nested.file"]),
                ("other.file", &[]),
                ("some.thing", &[]),
                ("some.thing.else", &[]),
            ],
        );
    }

    // upstream: StarTrie > multiple child patterns matching the parent itself
    #[test]
    fn multiple_child_patterns_matching_the_parent_itself() {
        let t = ExplorerFileNestingTrie::new(&[(
            "*".into(),
            s(&["${basename}.*", "${basename}*.${extname}"]),
        )]);
        let nesting1 = t.nest(
            &s(&[
                "Button.tsx",
                "Button.test.tsx",
                "Button.module.css",
                "ButtonGroup.tsx",
                "other.ts",
            ]),
            "mydir",
        );
        assert_nests(
            &nesting1,
            &[
                (
                    "Button.tsx",
                    &["Button.test.tsx", "Button.module.css", "ButtonGroup.tsx"],
                ),
                ("other.ts", &[]),
            ],
        );
    }

    // upstream: StarTrie > dirname matching
    #[test]
    fn dirname_matching() {
        let t = ExplorerFileNestingTrie::new(&[("index.ts".into(), s(&["${dirname}.ts"]))]);
        let nesting1 = t.nest(
            &s(&["otherFile.ts", "MyComponent.ts", "index.ts"]),
            "MyComponent",
        );
        assert_nests(
            &nesting1,
            &[("index.ts", &["MyComponent.ts"]), ("otherFile.ts", &[])],
        );
    }
}
