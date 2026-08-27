//! Profile matching for the discovery board.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub tags: Vec<String>,
    pub city: String,
    pub blocked: Vec<String>,
}

impl Profile {
    pub fn new(name: &str, tags: &[&str], city: &str) -> Self {
        Self {
            name: name.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            city: city.to_string(),
            blocked: Vec::new(),
        }
    }

    pub fn block(&mut self, name: &str) {
        self.blocked.push(name.to_string());
    }
}

/// The best candidate for `me`, as shown on the discovery board.
pub fn best_match<'a>(me: &Profile, candidates: &'a [Profile]) -> Option<&'a Profile> {
    candidates.iter().max_by_key(|c| affinity_score(me, c))
}

/// Affinity between two profiles: shared tags, a city bonus and a bonus for well-described pairs.
pub fn affinity_score(first: &Profile, right: &Profile) -> u32 {
    let shared = first
        .tags
        .iter()
        .filter(|tag| right.tags.contains(tag))
        .count() as u32;
    let city = if first.city == right.city { 3 } else { 0 };
    let described = if first.tags.len() >= 3 { 1 } else { 0 };
    shared * 2 + city + described
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_tags_and_city_add_up() {
        let a = Profile::new("ana", &["rust", "chess"], "lima");
        let b = Profile::new("bo", &["chess"], "lima");
        assert_eq!(affinity_score(&a, &b), 5);
    }

    #[test]
    fn best_match_prefers_the_highest_score() {
        let me = Profile::new("me", &["rust"], "lima");
        let candidates = vec![
            Profile::new("far", &["rust"], "oslo"),
            Profile::new("near", &["rust"], "lima"),
        ];
        assert_eq!(best_match(&me, &candidates).map(|p| p.name.as_str()), Some("near"));
    }

    #[test]
    fn board_scores_agree_for_both_parties() {
        let a = Profile::new("ana", &["rust", "chess", "jazz"], "lima");
        let b = Profile::new("bo", &["jazz", "rust"], "oslo");
        assert_eq!(affinity_score(&a, &b), affinity_score(&b, &a));
    }
}
