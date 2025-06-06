use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;

#[derive(Debug)]
/// A Trie for matching incoming MQTT messages with their subscribers
///
/// # Structure
/// Each node of the trie contains:
/// - a list of subscribers to the current topic
/// - a map of segments to trie nodes
///
/// As an example, if we subscribed a subscriber `"tedge-mapper"` to topic
/// `c8y/s/us`, the structure would look like:
///
/// ```text
/// {
///     "c8y": {
///         subscribers: [],
///         sub_nodes: {
///             "s": {
///                 subscribers: [],
///                 sub_nodes: {
///                     "us": {
///                         subscribers: ["tedge-mapper"],
///                         sub_nodes: {}
///                     }
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// In practice, this is achieved by having a root node that always has no
/// subscribers. What is shown above is the `sub_nodes` field of the root node.
///
/// # Subscription management
/// One of the requirements for the tedge MQTT actor is to manage subscriptions
/// from a bunch of clients and process them as a single MQTT channel client.
/// This means the actor maintains a minimal set of MQTT topic subscriptions to
/// cover all possible messages to its clients.
///
/// For example, if `client-a` subscribes to `a/b/c` and `client-b` subscribes
/// to `a/#`, the actor should subscribe to `a/#` only, since this wildcard
/// topic captures all messages on `a/b/c`, so we don't require a separate
/// subscription.
///
/// To allow the actor to subscribe/unsubscribe when appropriate,
/// [MqtTrie::insert] and [MqtTrie::remove] both return [Diff] objects. This
/// gives a list of topics that need subscribing to/unsubscribing from
/// following.
///
/// Here are some examples of diffs that are returned
///
/// ```
/// # use tedge_mqtt_ext::trie::*;
///
/// let mut t = MqtTrie::default();
/// // First subscriber -> subscribe to that topic
/// assert_eq!(t.insert("a/b", 1), Diff { subscribe: vec!["a/b".into()], unsubscribe: vec![] });
/// // Another subscriber to the same topics -> don't need to change subscriptions
/// assert_eq!(t.insert("a/b", 2), Diff { subscribe: vec![], unsubscribe: vec![] });
/// // Subscriber to a different topic -> subscribe to that topic
/// assert_eq!(t.insert("a", 1), Diff { subscribe: vec!["a".into()], unsubscribe: vec![] });
/// // Subscriber to a segment wildcard -> subscribe to that topic, unsubscribe from static topic
/// assert_eq!(t.insert("a/+", 1), Diff { subscribe: vec!["a/+".into()], unsubscribe: vec!["a/b".into()] });
/// // Subscriber to a wildcard -> subscribe to that topic and unsubscribe from the matching ones
/// // Don't unsubscribe from the already unsubscribed a/b topic though
/// assert_eq!(t.insert("#", 1), Diff { subscribe: vec!["#".into()], unsubscribe: vec!["a".into(), "a/+".into()] });
/// ```
///
/// It is still possible to end up with overlapping subscriptions via this
/// method. For instance, `a/+/c` and `a/b/+` both subscribe to messages on
/// `a/b/c`, but aren't overlapping. Currently, [MqtTrie] handles this by
/// subscribing to both `a/+/c` and `a/b/+`.
pub struct MqtTrie<T> {
    root: TrieNode<T>,
}

impl<T> Default for MqtTrie<T> {
    fn default() -> Self {
        Self {
            root: <_>::default(),
        }
    }
}

#[derive(Debug)]
pub struct Diff {
    pub subscribe: Vec<String>,
    pub unsubscribe: Vec<String>,
}

impl<T: Debug + Eq> MqtTrie<T> {
    /// Queries the trie for people subscribed to a given topic
    pub fn matches<'a>(&'a self, topic: &str) -> Vec<&'a T> {
        let mut nodes = Vec::new();
        self.root.matches(Some(topic), &mut nodes);
        nodes
    }

    /// Add a new subscriber
    pub fn insert(&mut self, topic: &str, subscriber: T) -> Diff {
        self.root.insert(topic, subscriber)
    }

    /// Removes an existing subscription
    pub fn remove(&mut self, topic: &str, id: &T) -> Diff {
        self.root.remove(topic, id)
    }
}

#[derive(Debug)]
struct TrieNode<T> {
    subscribers: Vec<T>,
    sub_nodes: HashMap<String, TrieNode<T>>,
}

impl<T> Default for TrieNode<T> {
    fn default() -> Self {
        Self {
            subscribers: <_>::default(),
            sub_nodes: <_>::default(),
        }
    }
}

impl Diff {
    fn empty() -> Self {
        Self {
            subscribe: vec![],
            unsubscribe: vec![],
        }
    }

    fn with_topic_prefix(self, prefix: &str) -> Self {
        Self {
            subscribe: self
                .subscribe
                .into_iter()
                .map(|t| format!("{prefix}/{t}"))
                .collect(),
            unsubscribe: self
                .unsubscribe
                .into_iter()
                .map(|t| format!("{prefix}/{t}"))
                .collect(),
        }
    }
}

impl PartialEq for Diff {
    fn eq(&self, other: &Self) -> bool {
        let mut s = HashSet::new();
        s.extend(&self.subscribe);
        let mut os = HashSet::new();
        os.extend(&other.subscribe);
        let mut us = HashSet::new();
        us.extend(&self.unsubscribe);
        let mut ous = HashSet::new();
        ous.extend(&other.unsubscribe);

        s == os && us == ous
    }
}

#[derive(PartialEq)]
struct RankTopic<'a>(&'a str);

impl PartialOrd for RankTopic<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;

        #[derive(PartialEq, Clone, Copy, Debug)]
        /// The current higher ranked subscription
        enum Winner {
            This,
            Other,
        }

        let mut current_winner = None;
        let mut self_segs = self.0.split("/");
        let mut other_segs = other.0.split("/");
        loop {
            let self_seg = self_segs.next();
            let other_seg = other_segs.next();
            match (self_seg, other_seg, current_winner) {
                (Some(a), Some(b), _) if a == b => continue,
                (Some("#"), Some(_), Some(Winner::Other)) => break None,
                (Some("#"), Some(_), _) => break Some(Ordering::Greater),
                (Some(_), Some("#"), Some(Winner::This)) => break None,
                (Some(_), Some("#"), _) => break Some(Ordering::Less),
                (Some("+"), Some(_), Some(Winner::Other)) => break None,
                (Some("+"), Some(_), _) => current_winner = Some(Winner::This),
                (Some(_), Some("+"), Some(Winner::This)) => break None,
                (Some(_), Some("+"), _) => current_winner = Some(Winner::Other),
                (Some(_), Some(_), _) => break None,
                (None, Some("#"), Some(Winner::This)) => break None,
                (None, Some("#"), _) => break Some(Ordering::Less),
                (None, Some(_), _) => break None,
                (Some("#"), None, Some(Winner::Other)) => break None,
                (Some("#"), None, _) => break Some(Ordering::Greater),
                (Some(_), None, _) => break None,
                (None, None, Some(Winner::This)) => break Some(Ordering::Greater),
                (None, None, Some(Winner::Other)) => break Some(Ordering::Less),
                (None, None, None) => break Some(Ordering::Equal),
            }
        }
    }
}

impl RankTopic<'_> {
    fn is_ranked_higher_than(&self, topics: &[impl AsRef<str>]) -> bool {
        use std::cmp::Ordering;

        !topics.iter().any(|t| {
            matches!(
                self.partial_cmp(&RankTopic(t.as_ref())),
                Some(Ordering::Less)
            )
        })
    }
}

fn remove_unneeded_topics(topics: &[impl AsRef<str>]) -> Vec<String> {
    use std::cmp::Ordering;

    let mut outranked = HashSet::new();
    let mut valid_topics = Vec::new();
    'outer: for i in 0..topics.len() {
        if outranked.contains(&i) {
            continue;
        }
        for j in (i + 1)..topics.len() {
            if outranked.contains(&j) {
                continue;
            }
            match RankTopic(topics[i].as_ref()).partial_cmp(&RankTopic(topics[j].as_ref())) {
                Some(Ordering::Greater | Ordering::Equal) => {
                    outranked.insert(j);
                }
                Some(Ordering::Less) => {
                    outranked.insert(i);
                    continue 'outer;
                }
                None => (),
            }
        }
        valid_topics.push(topics[i].as_ref().to_owned());
    }
    valid_topics
}

impl<T: Debug + Eq> TrieNode<T> {
    fn subscribers_matching(&self, topic: &str) -> Vec<String> {
        match topic.split_once("/") {
            Some(("+", rest)) => {
                if self.sub_nodes.contains_key("#") {
                    vec![]
                } else {
                    self.sub_nodes
                        .iter()
                        .flat_map(|(key, node)| {
                            node.subscribers_matching(rest)
                                .into_iter()
                                .map(move |t| format!("{key}/{t}"))
                        })
                        .collect()
                }
            }
            Some((head, rest)) => {
                if self.sub_nodes.contains_key("#") {
                    vec![]
                } else {
                    let mut key = "+";
                    self.sub_nodes
                        .get("+")
                        .or_else(|| {
                            key = head;
                            self.sub_nodes.get(head)
                        })
                        .map_or_else(Vec::new, |node| {
                            node.subscribers_matching(rest)
                                .into_iter()
                                .map(|t| format!("{key}/{t}"))
                                .collect()
                        })
                }
            }
            None if topic == "#" => self.subscribers(),
            None => self
                .sub_nodes
                .get(topic)
                .filter(|node| node.has_subscribers())
                .map(|_| topic.to_owned())
                .into_iter()
                .collect(),
        }
    }

    fn subscribers(&self) -> Vec<String> {
        if self.sub_nodes.contains_key("#") {
            vec!["#".to_owned()]
        } else {
            self.sub_nodes
                .iter()
                .flat_map(|(key, node)| {
                    let mut subs: Vec<_> = node
                        .subscribers()
                        .into_iter()
                        .map(|t| format!("{key}/{t}"))
                        .collect();
                    if node.has_subscribers() {
                        subs.push(key.clone());
                    }
                    subs
                })
                .collect()
        }
    }

    fn remove(&mut self, topic: &str, id: &T) -> Diff {
        match topic.split_once("/") {
            Some((head, rest)) => {
                if let Some(target) = self.sub_nodes.get_mut(head) {
                    let diff = target.remove(rest, id);
                    let current_node_subscribed_to = target.has_subscribers();
                    if !target.is_active() {
                        self.sub_nodes.remove(head);
                        if head == "+" {
                            // We can safely discard diff.subscribe at this
                            // point since no subscriptions exist (`!target.is_active()`)
                            return Diff {
                                subscribe: self
                                    .sub_nodes
                                    .iter()
                                    .flat_map(|(head, node)| {
                                        remove_unneeded_topics(&node.subscribers_matching(rest))
                                            .into_iter()
                                            .map(move |topic| format!("{head}/{topic}"))
                                    })
                                    .collect(),
                                unsubscribe: diff
                                    .unsubscribe
                                    .into_iter()
                                    .map(|topic| format!("{head}/{topic}"))
                                    .collect(),
                            };
                        }
                    }
                    if self.sub_nodes.contains_key("#") {
                        Diff::empty()
                    } else {
                        let mut diff = diff.with_topic_prefix(head);
                        if rest == "#"
                            && !self.sub_nodes.contains_key("#")
                            && current_node_subscribed_to
                        {
                            diff.subscribe.push(head.to_owned());
                        }
                        diff
                    }
                } else {
                    Diff::empty()
                }
            }
            None => {
                if let Some(target) = self.sub_nodes.get_mut(topic) {
                    let i = target.subscribers.iter().position(|t| t == id).unwrap();
                    target.subscribers.remove(i);
                    if target.has_subscribers() {
                        Diff::empty()
                    } else {
                        let mut diff = Diff {
                            subscribe: vec![],
                            unsubscribe: vec![topic.to_owned()],
                        };
                        if !target.is_active() {
                            self.sub_nodes.remove(topic);
                            if topic == "+" {
                                diff.subscribe = self
                                    .sub_nodes
                                    .iter()
                                    .filter_map(|(key, node)| {
                                        if node.has_subscribers() {
                                            Some(key.to_owned())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                            } else if topic == "#" {
                                diff.subscribe = self.subscribers();
                            }
                        }
                        diff
                    }
                } else {
                    Diff::empty()
                }
            }
        }
    }

    // TODO this should only accept valid message topic, not containing wildcards
    fn matches<'a>(&'a self, topic: Option<&str>, nodes: &mut Vec<&'a T>) {
        if let Some(topic) = topic {
            let (head, rest) = match topic.split_once("/") {
                Some((head, rest)) => (head, Some(rest)),
                None => (topic, None),
            };

            if let Some(node) = self.sub_nodes.get(head) {
                node.matches(rest, nodes);
            }
            if let Some(node) = self.sub_nodes.get("+") {
                node.matches(rest, nodes);
            }
            if let Some(node) = self.sub_nodes.get("#") {
                node.matches(None, nodes);
            }
        } else {
            nodes.extend(&self.subscribers);
            if let Some(node) = self.sub_nodes.get("#") {
                nodes.extend(&node.subscribers);
            }
        }
    }

    fn is_active(&self) -> bool {
        !self.subscribers.is_empty() || !self.sub_nodes.is_empty()
    }

    fn has_subscribers(&self) -> bool {
        !self.subscribers.is_empty()
    }

    fn insert(&mut self, topic: &str, subscriber: T) -> Diff {
        // TODO clone strings less when getting entries
        match topic.split_once("/") {
            Some((head, rest)) => {
                let overlapping_subscribers = self.subscribers_matching(topic);
                let mut diff = self
                    .sub_nodes
                    .entry(head.to_owned())
                    .or_default()
                    .insert(rest, subscriber);
                if self.sub_nodes.contains_key("#") {
                    Diff::empty()
                } else {
                    diff = diff.with_topic_prefix(head);
                    if head == "+" {
                        diff.unsubscribe.extend(overlapping_subscribers);
                    } else {
                        diff.subscribe.retain(|t| {
                            RankTopic(t).is_ranked_higher_than(&overlapping_subscribers)
                        })
                    }
                    if rest == "#" {
                        if let Some(h) = self.sub_nodes.get(head) {
                            if h.has_subscribers() {
                                diff.unsubscribe.push(head.to_owned());
                            }
                        }
                    }
                    diff.unsubscribe = remove_unneeded_topics(&diff.unsubscribe);
                    diff
                }
            }
            None => {
                if let Some(entry) = self.sub_nodes.get_mut(topic) {
                    let already_subscribed = entry.has_subscribers();
                    entry.subscribers.push(subscriber);
                    if already_subscribed {
                        Diff::empty()
                    } else {
                        self.insert_diff_for(topic)
                    }
                } else {
                    self.sub_nodes.insert(
                        topic.to_owned(),
                        TrieNode {
                            subscribers: vec![subscriber],
                            ..<_>::default()
                        },
                    );
                    self.insert_diff_for(topic)
                }
            }
        }
    }

    fn insert_diff_for(&self, topic: &str) -> Diff {
        let wildcard_subscription_exists = match topic {
            "+" => self.sub_nodes.contains_key("#"),
            "#" => false,
            _ => {
                self.sub_nodes
                    .get("+")
                    .is_some_and(|node| node.has_subscribers())
                    || self.sub_nodes.contains_key("#")
                    || self
                        .sub_nodes
                        .get(topic)
                        .is_some_and(|node| node.sub_nodes.contains_key("#"))
            }
        };
        if wildcard_subscription_exists {
            Diff::empty()
        } else {
            let unsubscribe = match topic {
                "+" => self
                    .direct_sub_topics()
                    .into_iter()
                    .filter(|t| t != topic)
                    .collect(),
                "#" => self
                    .all_sub_topics()
                    .into_iter()
                    .filter(|t| t != topic)
                    .collect(),
                _ => vec![],
            };
            Diff {
                subscribe: vec![topic.to_owned()],
                unsubscribe: remove_unneeded_topics(&unsubscribe),
            }
        }
    }

    fn direct_sub_topics(&self) -> Vec<String> {
        let mut res = vec![];
        for (topic, node) in &self.sub_nodes {
            if node.has_subscribers() {
                res.push(topic.to_owned());
            }
        }
        res
    }

    fn all_sub_topics(&self) -> Vec<String> {
        let mut res = vec![];
        for (topic, node) in &self.sub_nodes {
            if node.has_subscribers() {
                res.push(topic.to_owned());
            }
            res.extend(
                node.all_sub_topics()
                    .into_iter()
                    .map(|s| format!("{topic}/{s}")),
            );
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    #[derive(Debug)]
    struct SubscribeTo(&'static str);

    impl PartialEq<SubscribeTo> for Diff {
        fn eq(&self, other: &SubscribeTo) -> bool {
            self.unsubscribe.is_empty()
                && self.subscribe.len() <= 1
                && self.subscribe.first().map(|s| s.as_str()) == Some(other.0)
        }
    }

    impl PartialEq<Option<Infallible>> for Diff {
        fn eq(&self, _: &Option<Infallible>) -> bool {
            self.unsubscribe.is_empty() && self.subscribe.is_empty()
        }
    }

    mod rank_topic {
        use super::*;
        use std::cmp::Ordering;

        #[test]
        fn single_segment_wildcard_ranks_higher_than_static_topic() {
            assert_eq!(
                RankTopic("a/+/c").partial_cmp(&RankTopic("a/b/c")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn static_topic_ranks_lower_than_single_wildcard() {
            assert_eq!(
                RankTopic("a/b/c").partial_cmp(&RankTopic("a/+/c")),
                Some(Ordering::Less)
            );
        }

        #[test]
        fn static_topic_ranks_lower_than_global_wildcard() {
            assert_eq!(
                RankTopic("a/b/c").partial_cmp(&RankTopic("a/#")),
                Some(Ordering::Less)
            );
        }

        #[test]
        fn global_wildcard_ranks_higher_than_segment_wildcard() {
            assert_eq!(
                RankTopic("a/#").partial_cmp(&RankTopic("a/+")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn matching_static_topics_rank_equally() {
            assert_eq!(
                RankTopic("a/b/c").partial_cmp(&RankTopic("a/b/c")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn matching_global_wildcard_topics_rank_equally() {
            assert_eq!(
                RankTopic("a/b/#").partial_cmp(&RankTopic("a/b/#")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn matching_segment_wildcard_topics_rank_equally() {
            assert_eq!(
                RankTopic("a/b/+").partial_cmp(&RankTopic("a/b/+")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn disjoint_static_topics_do_not_compare() {
            assert_eq!(RankTopic("a/b").partial_cmp(&RankTopic("b/c")), None);
        }

        #[test]
        fn partially_disjoint_static_topics_do_not_compare() {
            assert_eq!(RankTopic("a/b/c").partial_cmp(&RankTopic("a/b/d")), None);
        }

        // (Some(_), Some("+"), Some(Winner::This)) => break None
        #[test]
        fn topics_with_disjoint_wildcards_do_not_compare_bis() {
            assert_eq!(
                RankTopic("+/a").partial_cmp(&RankTopic("a/+")),
                None
            )
        }

        #[test]
        fn topic_with_more_wildcards_ranks_higher() {
            assert_eq!(
                RankTopic("a/+/+/d").partial_cmp(&RankTopic("a/+/c/d")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn topics_with_disjoint_wildcards_do_not_compare() {
            assert_eq!(
                RankTopic("a/b/+/d").partial_cmp(&RankTopic("a/+/c/d")),
                None
            );
        }

        #[test]
        fn global_wildcard_suffix_ranks_higher_than_unsuffixed_topic() {
            assert_eq!(
                RankTopic("a/#").partial_cmp(&RankTopic("a")),
                Some(Ordering::Greater)
            )
        }

        //(None, Some("#"), Some(Winner::This)) => break None,
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix() {
            assert_eq!(
                RankTopic("+").partial_cmp(&RankTopic("a/#")),
                None
            );
        }

        // (Some("#"), None, Some(Winner::Other)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_bis() {
            assert_eq!(
                RankTopic("a/#").partial_cmp(&RankTopic("+")),
                None
            );
        }

        // (Some(_), Some("#"), Some(Winner::This)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_ter() {
            assert_eq!(
                RankTopic("+/a").partial_cmp(&RankTopic("a/#")),
                None
            );
        }

        // (Some("#"), Some(_), Some(Winner::Other)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_4() {
            assert_eq!(
                RankTopic("a/#").partial_cmp(&RankTopic("+/a")),
                None
            );
        }

    }

    mod insert {
        use super::*;

        #[test]
        fn subscribes_to_static_topics() {
            let mut t = MqtTrie::default();

            assert_eq!(t.insert("a/b/c", 1), SubscribeTo("a/b/c"));
        }

        #[test]
        fn subscribes_to_segment_wildcards() {
            let mut t = MqtTrie::default();

            assert_eq!(t.insert("a/+", 1), SubscribeTo("a/+"));
        }

        #[test]
        fn subscribes_to_mid_segment_wildcards() {
            let mut t = MqtTrie::default();

            assert_eq!(t.insert("a/+/c", 1), SubscribeTo("a/+/c"));
        }

        #[test]
        fn subscribes_to_global_wildcards() {
            let mut t = MqtTrie::default();

            assert_eq!(t.insert("a/#", 1), SubscribeTo("a/#"));
        }

        #[test]
        fn subscribes_to_non_overlapping_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);

            assert_eq!(t.insert("a/+", 1), SubscribeTo("a/+"));
        }

        #[test]
        fn unsubscribes_when_wildcard_supersedes_existing_subscription() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);
            t.insert("a/+", 2);

            assert_eq!(
                t.insert("a/#", 3),
                Diff {
                    unsubscribe: vec!["a/b/c".to_owned(), "a/+".to_owned()],
                    subscribe: vec!["a/#".to_owned()]
                }
            );
        }

        #[test]
        fn does_not_resubscribe_to_existing_static_topic() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);

            assert_eq!(t.insert("a/b/c", 2), None);
        }

        #[test]
        fn does_not_subscribe_when_topic_matches_existing_end_segment_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/b/+", 1);

            assert_eq!(t.insert("a/b/c", 2), None);
        }

        #[test]
        fn does_not_subscribe_when_topic_matches_existing_mid_segment_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c", 1);

            assert_eq!(t.insert("a/b/c", 2), None);
        }

        #[test]
        fn does_not_subscribe_to_topic_when_topic_matches_existing_global_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.insert("a/b/c", 2), None);
        }

        #[test]
        fn does_not_subscribe_to_wildcard_when_higher_global_wildcard_exists() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.insert("a/b/#", 2), None);
        }

        #[test]
        fn subscribes_to_topic_disjoint_from_existing_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.insert("b/c", 2), SubscribeTo("b/c"));
        }

        #[test]
        fn unsubscribes_superseded_topics_when_mid_segment_wildcard_is_subscribed_to() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c/d", 1);

            assert_eq!(
                t.insert("a/+/c/d", 2),
                Diff {
                    subscribe: vec!["a/+/c/d".to_owned()],
                    unsubscribe: vec!["a/b/c/d".to_owned()],
                }
            );
        }

        #[test]
        fn subscribes_when_first_subscriber_is_inserted_to_existing_wildcard_trie_node() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c", 1);

            // In this case, a/+ exists in the trie already, but isn't yet
            // subscribed to, hence this should add a subscription
            assert_eq!(t.insert("a/+", 1), SubscribeTo("a/+"));
        }

        #[test]
        fn subscribes_when_first_subscriber_is_inserted_to_existing_static_trie_node() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);

            assert_eq!(t.insert("a/b", 1), SubscribeTo("a/b"));
        }

        #[test]
        fn subscribes_when_a_wildcard_is_superseded_by_another_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c/d", 1);

            assert_eq!(
                t.insert("a/+/+/d", 2),
                Diff {
                    subscribe: vec!["a/+/+/d".to_owned()],
                    unsubscribe: vec!["a/+/c/d".to_owned()],
                }
            );
        }

        #[test]
        fn does_not_subscribe_when_a_wildcard_is_already_superseded() {
            let mut t = MqtTrie::default();
            t.insert("a/+/+/d", 1);

            assert_eq!(t.insert("a/b/+/d", 2), Diff::empty());
            let mut t = MqtTrie::default();
            t.insert("a/b/+/+", 1);

            assert_eq!(t.insert("a/b/+/d", 2), Diff::empty());
        }

        #[test]
        fn unsubscribes_only_to_subscribed_topics() {
            let mut t = MqtTrie::default();
            t.insert("a", 1);
            t.insert("a/+", 1);
            t.insert("a/b", 1);

            assert_eq!(
                t.insert("#", 1),
                Diff {
                    subscribe: vec!["#".into()],
                    unsubscribe: vec!["a".into(), "a/+".into()]
                }
            );
        }

        #[test]
        fn subscribing_to_global_wildcard_unsubscribes_parent_topic() {
            let mut t = MqtTrie::default();
            t.insert("a", 1);

            assert_eq!(
                t.insert("a/#", 2),
                Diff {
                    subscribe: vec!["a/#".into()],
                    unsubscribe: vec!["a".into()],
                }
            );
        }

        #[test]
        fn does_not_subscribe_parent_of_existing_global_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.insert("a", 2), Diff::empty());
        }
    }

    mod remove {
        use super::*;

        #[test]
        fn basic_unsubscription_calls_unsubscribe() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);

            assert_eq!(
                t.remove("a/b", &1),
                Diff {
                    subscribe: vec![],
                    unsubscribe: vec!["a/b".to_owned()],
                }
            )
        }

        #[test]
        fn removing_one_of_multiple_subscribers_does_not_unsubscribe() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);
            t.insert("a/b", 2);

            assert_eq!(t.remove("a/b", &1), Diff::empty())
        }

        #[test]
        fn removing_wildcard_topic_unsubscribes() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c/#", 1);

            assert_eq!(
                t.remove("a/+/c/#", &1),
                Diff {
                    unsubscribe: vec!["a/+/c/#".to_owned()],
                    subscribe: vec![],
                }
            );
        }

        #[test]
        fn removing_wildcard_topic_resubscribes() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);
            t.insert("a/+/c", 2);

            assert_eq!(
                t.remove("a/+/c", &2),
                Diff {
                    unsubscribe: vec!["a/+/c".to_owned()],
                    subscribe: vec!["a/b/c".to_owned()],
                }
            );
        }

        #[test]
        fn removing_global_wildcard_topic_resubscribes() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);
            t.insert("a/#", 2);

            assert_eq!(
                t.remove("a/#", &2),
                Diff {
                    unsubscribe: vec!["a/#".to_owned()],
                    subscribe: vec!["a/b/c".to_owned()],
                }
            );
        }

        #[test]
        fn unsubscribing_to_global_wildcard_resubscribes_parent_topic() {
            let mut t = MqtTrie::default();
            t.insert("a", 1);
            t.insert("a/#", 2);

            assert_eq!(
                t.remove("a/#", &2),
                Diff {
                    unsubscribe: vec!["a/#".into()],
                    subscribe: vec!["a".into()],
                }
            );
        }

        #[test]
        fn removing_double_segment_wildcard_resubscribes_single_segment_but_not_static_topic() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c/d", 1);
            t.insert("a/+/c/d", 2);
            t.insert("a/+/+/d", 3);

            assert_eq!(
                t.remove("a/+/+/d", &3),
                Diff {
                    unsubscribe: vec!["a/+/+/d".to_owned()],
                    subscribe: vec!["a/+/c/d".to_owned()],
                }
            );
        }

        #[test]
        fn unsubscribing_from_topic_masked_by_global_wildcard_subscription_changes_nothing() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);
            t.insert("a/b/c", 2);

            assert_eq!(t.remove("a/b/c", &2), Diff::empty());
        }

        #[test]
        fn unsubscribing_from_a_non_subscribed_topic_changes_nothing() {
            let mut t = MqtTrie::default();

            assert_eq!(t.remove("a/b/c", &1), Diff::empty());
        }

        #[test]
        fn unsubscribing_from_an_end_segment_wildcard_resubscribes_to_existing_topics() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);
            t.insert("a/+", 2);

            assert_eq!(
                t.remove("a/+", &2),
                Diff {
                    unsubscribe: vec!["a/+".into()],
                    subscribe: vec!["a/b".into()],
                }
            );
        }

        #[test]
        fn unsubscribing_from_an_end_segment_wildcard_does_not_resubscribe_to_non_subscribed_topics(
        ) {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);
            t.insert("a/+", 2);

            assert_eq!(
                t.remove("a/+", &2),
                Diff {
                    unsubscribe: vec!["a/+".into()],
                    subscribe: vec![],
                }
            );
        }
    }

    mod matches {
        use super::*;

        #[test]
        fn basic_topic_subscription() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);

            assert_eq!(t.matches("a/b/c"), vec![&1]);
        }

        #[test]
        fn end_segment_wildcard_subscription() {
            let mut t = MqtTrie::default();
            t.insert("a/b/+", 1);

            assert_eq!(t.matches("a/b/c"), vec![&1]);
        }

        #[test]
        fn middle_segment_wildcard_subscription() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c", 1);

            assert_eq!(t.matches("a/b/c"), vec![&1]);
        }

        #[test]
        fn does_not_return_non_matching_subscriber() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);
            t.insert("b/c", 2);

            assert_eq!(t.matches("b/c"), vec![&2]);
        }

        #[test]
        fn global_wildcard_subscription() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.matches("a/b/c"), vec![&1]);
        }

        #[test]
        fn matches_all_subscriptions() {
            let mut t = MqtTrie::default();
            // Using the subscription topic as the subscriber field
            // so any assertion errors are easier to parse
            t.insert("a/#", "a/#");
            t.insert("a/b/#", "a/b/#");
            t.insert("a/b/+", "a/b/+");
            t.insert("a/b/c", "a/b/c");

            assert_eq!(
                sorted_matches(&t, "a/b/c"),
                [&"a/#", &"a/b/#", &"a/b/+", &"a/b/c"]
            );
        }

        #[test]
        fn global_wildcard_suffix_matches_parent() {
            let mut t = MqtTrie::default();
            t.insert("a/#", "a/#");

            assert_eq!(sorted_matches(&t, "a"), [&"a/#"]);
        }

        fn sorted_matches<'a, T: Ord + Debug>(t: &'a MqtTrie<T>, topic: &str) -> Vec<&'a T> {
            let mut matches = t.matches(topic);
            matches.sort();
            matches
        }
    }
}
