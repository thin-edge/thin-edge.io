use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::ops::AddAssign;

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
/// [MqtTrie::insert] and [MqtTrie::remove] both return [SubscriptionDiff]
/// objects. This returns the subscribe/unsubscribe requests that need to be
/// made to the MQTT broker following the internal subscription change.
///
/// Here are some examples of diffs that are returned
///
/// ```
/// # use tedge_mqtt_ext::trie::*;
///
/// let mut t = MqtTrie::default();
/// // First subscriber -> subscribe to that topic
/// assert_eq!(t.insert("a/b", 1), SubscriptionDiff { subscribe: ["a/b".into()].into(), unsubscribe: [].into() });
/// // Another subscriber to the same topics -> don't need to change subscriptions
/// assert_eq!(t.insert("a/b", 2), SubscriptionDiff { subscribe: [].into(), unsubscribe: [].into() });
/// // Subscriber to a different topic -> subscribe to that topic
/// assert_eq!(t.insert("a", 1), SubscriptionDiff { subscribe: ["a".into()].into(), unsubscribe: [].into() });
/// // Subscriber to a segment wildcard -> subscribe to that topic, unsubscribe from static topic
/// assert_eq!(t.insert("a/+", 1), SubscriptionDiff { subscribe: ["a/+".into()].into(), unsubscribe: ["a/b".into()].into() });
/// // Subscriber to a wildcard -> subscribe to that topic and unsubscribe from the matching ones
/// // Don't unsubscribe from the already unsubscribed a/b topic though
/// assert_eq!(t.insert("#", 1), SubscriptionDiff { subscribe: ["#".into()].into(), unsubscribe: ["a".into(), "a/+".into()].into() });
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionDiff {
    pub subscribe: HashSet<String>,
    pub unsubscribe: HashSet<String>,
}

impl<T: Debug + Eq> MqtTrie<T> {
    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    /// Queries the trie for people subscribed to a given topic
    pub fn matches<'a>(&'a self, topic: &str) -> Vec<&'a T> {
        let mut nodes = Vec::new();
        self.root.matches(Some(topic), &mut nodes);
        nodes
    }

    /// Add a new subscriber
    pub fn insert(&mut self, topic: &str, subscriber: T) -> SubscriptionDiff {
        self.root.insert(topic, subscriber)
    }

    /// Removes an existing subscription
    pub fn remove(&mut self, topic: &str, id: &T) -> SubscriptionDiff {
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

impl AddAssign for SubscriptionDiff {
    fn add_assign(&mut self, rhs: Self) {
        self.subscribe.extend(rhs.subscribe);
        self.unsubscribe.extend(rhs.unsubscribe);
        self.simplify()
    }
}

impl SubscriptionDiff {
    pub fn empty() -> Self {
        Self {
            subscribe: <_>::default(),
            unsubscribe: <_>::default(),
        }
    }

    pub fn new(
        subscribe: &mqtt_channel::TopicFilter,
        unsubscribe: &mqtt_channel::TopicFilter,
    ) -> Self {
        let mut diff = Self {
            subscribe: subscribe.patterns().iter().cloned().collect(),
            unsubscribe: unsubscribe.patterns().iter().cloned().collect(),
        };
        diff.simplify();
        diff
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

    fn simplify(&mut self) {
        let overlap = self
            .subscribe
            .intersection(&self.unsubscribe)
            .cloned()
            .collect::<Vec<_>>();
        for topic in overlap {
            self.subscribe.remove(&topic);
            self.unsubscribe.remove(&topic);
        }
    }
}

#[derive(PartialEq)]
/// A partial ordering for topics, used to remove overlapping subscriptions
///
/// The ordering is defined such that:
/// - a > b iff a is a wildcard topic fully containing b
/// - a == b iff a and b are identical
/// - a < b iff b > a
/// - topics that are disjoint are not comparable
///
/// The result of this is that a <= b implies that any topic accepted by filter
/// a is also accepted by filter b.
///
/// # Examples
/// "a/#" > "a/b/c"
/// "a/+" > "a/b"
/// "a/b" == "a/b"
/// "a" < "a/#"
/// "a" < "#"
/// "a/+" does not compare to "a/b/c"
/// "a/+/c" does not compare to "a/b/+"
/// "a/b" does not compare to "c/d"
struct RankTopicFilter<'a>(&'a str);

impl PartialOrd for RankTopicFilter<'_> {
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
                // Identical segments, keep searching for differences
                (Some(a), Some(b), _) if a == b => continue,

                // # > anything other than #
                (Some("#"), Some(_), Some(Winner::Other)) => break None,
                (Some("#"), Some(_), _) => break Some(Ordering::Greater),
                (Some(_), Some("#"), Some(Winner::This)) => break None,
                (Some(_), Some("#"), _) => break Some(Ordering::Less),

                // + > static segment, but keep going as there may be more segments
                (Some("+"), Some(_), Some(Winner::Other)) => break None,
                (Some("+"), Some(_), _) => current_winner = Some(Winner::This),
                (Some(_), Some("+"), Some(Winner::This)) => break None,
                (Some(_), Some("+"), _) => current_winner = Some(Winner::Other),

                // a/.. and b/.. are not comparable
                (Some(_), Some(_), _) => break None,

                // a/# > a
                (None, Some("#"), Some(Winner::This)) => break None,
                (None, Some("#"), _) => break Some(Ordering::Less),
                (None, Some(_), _) => break None,
                (Some("#"), None, Some(Winner::Other)) => break None,
                (Some("#"), None, _) => break Some(Ordering::Greater),
                (Some(_), None, _) => break None,

                // Both filters have the same length
                (None, None, Some(Winner::This)) => break Some(Ordering::Greater),
                (None, None, Some(Winner::Other)) => break Some(Ordering::Less),
                (None, None, None) => break Some(Ordering::Equal),
            }
        }
    }
}

impl RankTopicFilter<'_> {
    fn is_ranked_higher_than<S: AsRef<str>>(&self, filters: impl IntoIterator<Item = S>) -> bool {
        use std::cmp::Ordering;

        !filters.into_iter().any(|t| {
            matches!(
                self.partial_cmp(&RankTopicFilter(t.as_ref())),
                Some(Ordering::Less)
            )
        })
    }
}

fn remove_unneeded_topics(topics: &[impl AsRef<str>]) -> HashSet<String> {
    use std::cmp::Ordering;

    let mut outranked = HashSet::new();
    let mut minimal_set = HashSet::new();
    'outer: for i in 0..topics.len() {
        if outranked.contains(&i) {
            continue;
        }
        for j in (i + 1)..topics.len() {
            if outranked.contains(&j) {
                continue;
            }
            match RankTopicFilter(topics[i].as_ref())
                .partial_cmp(&RankTopicFilter(topics[j].as_ref()))
            {
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
        minimal_set.insert(topics[i].as_ref().to_owned());
    }
    minimal_set
}

impl<T: Debug + Eq> TrieNode<T> {
    fn is_empty(&self) -> bool {
        self.sub_nodes.is_empty() && self.subscribers.is_empty()
    }

    fn subscribed_topics_matching(&self, topic_suffix: &str) -> Vec<String> {
        self.subscribed_topics_matching_inner(topic_suffix, None)
    }
    fn subscribed_topics_matching_inner(
        &self,
        topic_suffix: &str,
        ord: Option<std::cmp::Ordering>,
    ) -> Vec<String> {
        use std::cmp::Ordering;
        match topic_suffix.split_once("/") {
            Some(("+", rest)) if ord != Some(Ordering::Less) => {
                if self.sub_nodes.contains_key("#") {
                    vec!["#".into()]
                } else {
                    self.sub_nodes
                        .iter()
                        .flat_map(|(key, node)| {
                            node.subscribed_topics_matching_inner(rest, Some(Ordering::Greater))
                                .into_iter()
                                .map(move |t| format!("{key}/{t}"))
                        })
                        .collect()
                }
            }
            Some((head, rest)) => {
                if self.sub_nodes.contains_key("#") {
                    vec!["#".into()]
                } else {
                    let mut matching_nodes = Vec::new();
                    if ord != Some(Ordering::Greater) {
                        if let Some(node) = self.sub_nodes.get("+") {
                            matching_nodes.extend(
                                node.subscribed_topics_matching_inner(rest, Some(Ordering::Less))
                                    .into_iter()
                                    .map(|t| format!("+/{t}")),
                            );
                        }
                    }
                    if let Some(node) = self.sub_nodes.get(head) {
                        matching_nodes.extend(
                            node.subscribed_topics_matching(rest)
                                .into_iter()
                                .map(|t| format!("{head}/{t}")),
                        );
                    }
                    matching_nodes
                }
            }
            None if topic_suffix == "#" => self.subscribed_topics(),
            None if topic_suffix == "+" && ord != Some(Ordering::Less) => self
                .sub_nodes
                .iter()
                .filter(|(_, node)| node.has_subscribers())
                .filter(|(_, node)| !node.sub_nodes.contains_key("#"))
                .map(|(key, _)| key.clone())
                .collect(),
            None if self.sub_nodes.contains_key("#") => vec!["#".into()],
            None => {
                let mut matching_nodes: Vec<_> = self
                    .sub_nodes
                    .get(topic_suffix)
                    .filter(|node| node.has_subscribers())
                    .filter(|node| !node.sub_nodes.contains_key("#"))
                    .map(|_| topic_suffix.to_owned())
                    .into_iter()
                    .collect();
                if ord != Some(Ordering::Greater) {
                    matching_nodes.extend(
                        self.sub_nodes
                            .get("+")
                            .filter(|node| node.has_subscribers())
                            .filter(|node| !node.sub_nodes.contains_key("#"))
                            .map(|_| "+".into()),
                    );
                }
                matching_nodes
            }
        }
    }

    fn subscribed_topics(&self) -> Vec<String> {
        if self.sub_nodes.contains_key("#") {
            vec!["#".to_owned()]
        } else {
            self.sub_nodes
                .iter()
                .flat_map(|(key, node)| {
                    let mut subs: Vec<_> = node
                        .subscribed_topics()
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

    fn remove(&mut self, topic_suffix: &str, id: &T) -> SubscriptionDiff {
        match topic_suffix.split_once("/") {
            Some((head, rest)) => {
                if let Some(target) = self.sub_nodes.get_mut(head) {
                    let diff = target.remove(rest, id);
                    let current_node_subscribed_to = target.has_subscribers();
                    if !target.is_active() {
                        self.sub_nodes.remove(head);
                        if head == "+" {
                            // We can safely discard diff.subscribe at this
                            // point since no subscriptions exist (`!target.is_active()`)
                            return SubscriptionDiff {
                                subscribe: self
                                    .sub_nodes
                                    .iter_mut()
                                    .flat_map(|(head, node)| {
                                        remove_unneeded_topics(
                                            &node.subscribed_topics_matching(rest),
                                        )
                                        .into_iter()
                                        .filter(|topic| {
                                            RankTopicFilter(rest) >= RankTopicFilter(topic)
                                        })
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
                        SubscriptionDiff::empty()
                    } else {
                        let mut diff = diff.with_topic_prefix(head);
                        if rest == "#"
                            && !self.sub_nodes.contains_key("#")
                            && current_node_subscribed_to
                        {
                            diff.subscribe.insert(head.to_owned());
                        }
                        diff
                    }
                } else {
                    SubscriptionDiff::empty()
                }
            }
            None => {
                let has_sibling_global_wildcard = self.sub_nodes.contains_key("#");
                if let Some(target) = self.sub_nodes.get_mut(topic_suffix) {
                    let Some(i) = target.subscribers.iter().position(|t| t == id) else {
                        return SubscriptionDiff::empty();
                    };
                    target.subscribers.remove(i);
                    if target.has_subscribers() {
                        SubscriptionDiff::empty()
                    } else {
                        let mut diff = SubscriptionDiff {
                            subscribe: HashSet::new(),
                            unsubscribe: HashSet::from([topic_suffix.to_owned()]),
                        };
                        if !target.is_active() {
                            self.sub_nodes.remove(topic_suffix);
                            if topic_suffix == "+" && !has_sibling_global_wildcard {
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
                            } else if topic_suffix == "#" {
                                diff.subscribe.extend(self.subscribed_topics());
                            }
                        }
                        diff
                    }
                } else {
                    SubscriptionDiff::empty()
                }
            }
        }
    }

    fn matches<'a>(&'a self, topic_suffix: Option<&str>, nodes: &mut Vec<&'a T>) {
        if let Some(topic) = topic_suffix {
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
        !self.subscribers.is_empty() && !self.sub_nodes.contains_key("#")
    }

    fn insert(&mut self, topic_suffix: &str, subscriber: T) -> SubscriptionDiff {
        match topic_suffix.split_once("/") {
            Some((head, rest)) => {
                let overlapping_subscribers = self.subscribed_topics_matching(topic_suffix);
                let possible_overlapping_subscribers = self.subscribed_topics();

                let mut diff = self
                    .sub_nodes
                    .entry(head.to_owned())
                    .or_default()
                    .insert(rest, subscriber);
                if self.sub_nodes.contains_key("#") {
                    SubscriptionDiff::empty()
                } else {
                    diff = diff.with_topic_prefix(head);
                    if head == "+" {
                        if !diff.subscribe.is_empty() {
                            diff.unsubscribe.extend(
                                remove_unneeded_topics(&overlapping_subscribers)
                                    .into_iter()
                                    .filter(|t| {
                                        RankTopicFilter(topic_suffix) >= RankTopicFilter(t)
                                    }),
                            );
                        }
                    } else {
                        diff.subscribe.retain(|t| {
                            RankTopicFilter(t).is_ranked_higher_than(&overlapping_subscribers)
                        });
                    }
                    diff.unsubscribe.retain(|t| {
                        RankTopicFilter(t).is_ranked_higher_than(&possible_overlapping_subscribers)
                    });
                    if rest == "#" && !diff.subscribe.is_empty() {
                        let parent_subscribed = self
                            .sub_nodes
                            .get(head)
                            .is_some_and(|p| !p.subscribers.is_empty());
                        let parent_wildcard_subscribed =
                            self.sub_nodes.get("+").is_some_and(|p| p.has_subscribers());
                        if parent_subscribed && !parent_wildcard_subscribed {
                            diff.unsubscribe.insert(head.to_owned());
                        }
                    }
                    diff
                }
            }
            None => {
                if let Some(entry) = self.sub_nodes.get_mut(topic_suffix) {
                    let already_subscribed = !entry.subscribers.is_empty();
                    entry.subscribers.push(subscriber);
                    if already_subscribed {
                        SubscriptionDiff::empty()
                    } else {
                        self.insert_diff_for(topic_suffix)
                    }
                } else {
                    self.sub_nodes.insert(
                        topic_suffix.to_owned(),
                        TrieNode {
                            subscribers: vec![subscriber],
                            ..<_>::default()
                        },
                    );
                    self.insert_diff_for(topic_suffix)
                }
            }
        }
    }

    fn insert_diff_for(&self, topic_suffix: &str) -> SubscriptionDiff {
        let wildcard_subscription_exists = match topic_suffix {
            "+" => self.sub_nodes.contains_key("#"),
            "#" => false,
            _ => {
                self.sub_nodes
                    .get("+")
                    .is_some_and(|node| node.has_subscribers())
                    || self.sub_nodes.contains_key("#")
                    || self
                        .sub_nodes
                        .get(topic_suffix)
                        .is_some_and(|node| node.sub_nodes.contains_key("#"))
            }
        };
        if wildcard_subscription_exists {
            SubscriptionDiff::empty()
        } else {
            let unsubscribe = match topic_suffix {
                "+" => self.direct_sub_topics_except(topic_suffix),
                "#" => self.all_sub_topics_except(topic_suffix),
                _ => HashSet::new(),
            };
            SubscriptionDiff {
                subscribe: HashSet::from([topic_suffix.to_owned()]),
                unsubscribe,
            }
        }
    }

    // Direct sub topics are topics in the segment below, e.g. a/b/c and a/b/#
    // are direct sub-topics of a/b
    fn direct_sub_topics_except(&self, except: &str) -> HashSet<String> {
        let mut res = HashSet::new();
        for (topic, node) in &self.sub_nodes {
            if node.has_subscribers() && topic != except {
                res.insert(topic.to_owned());
            }
        }
        res
    }

    // All sub topics contains both the direct sub topics (see above) and
    // recursively finds the sub topics, e.g. a/b/c and a/b/c/d are both sub
    // topics of a/b as far as this method is concerned
    fn all_sub_topics_except(&self, except: &str) -> HashSet<String> {
        let mut res = HashSet::new();
        self.all_sub_topics_except_inner(except, &mut res, "");
        res
    }

    fn all_sub_topics_except_inner(&self, except: &str, res: &mut HashSet<String>, prefix: &str) {
        let possible_plus = self.sub_nodes.get_key_value("+");
        for (topic, node) in possible_plus.into_iter().chain(&self.sub_nodes) {
            let full_topic = format!("{prefix}{topic}");
            if node.has_subscribers()
                && full_topic != except
                && RankTopicFilter(&full_topic).is_ranked_higher_than(&*res)
            {
                res.insert(full_topic.clone());
            }
            node.all_sub_topics_except_inner(except, res, &format!("{full_topic}/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    #[derive(Debug)]
    struct SubscribeTo(&'static str);

    impl PartialEq<SubscribeTo> for SubscriptionDiff {
        fn eq(&self, other: &SubscribeTo) -> bool {
            self.unsubscribe.is_empty()
                && self.subscribe.len() <= 1
                && self.subscribe.iter().next().map(|s| s.as_str()) == Some(other.0)
        }
    }

    impl PartialEq<Option<Infallible>> for SubscriptionDiff {
        fn eq(&self, _: &Option<Infallible>) -> bool {
            self.unsubscribe.is_empty() && self.subscribe.is_empty()
        }
    }

    mod diff {
        use super::*;

        #[test]
        fn adding_to_an_empty_diff_preserves_all_values() {
            let mut target = SubscriptionDiff::empty();
            let rhs = SubscriptionDiff {
                subscribe: ["some/#".into()].into(),
                unsubscribe: ["some/topic".into()].into(),
            };
            target += rhs.clone();

            assert_eq!(target, rhs);
        }

        #[test]
        fn adding_an_empty_diff_to_a_nonempty_diff_preserves_all_values() {
            let original = SubscriptionDiff {
                subscribe: ["some/#".into()].into(),
                unsubscribe: ["some/topic".into()].into(),
            };
            let mut target = original.clone();
            target += SubscriptionDiff::empty();

            assert_eq!(target, original);
        }

        #[test]
        fn adding_two_disjoint_diffs_preserves_all_values() {
            let mut diff = SubscriptionDiff {
                subscribe: ["different/topic".into()].into(),
                unsubscribe: ["different/+".into()].into(),
            };
            let rhs = SubscriptionDiff {
                subscribe: ["some/#".into()].into(),
                unsubscribe: ["some/topic".into()].into(),
            };
            diff += rhs.clone();

            assert_eq!(
                diff,
                SubscriptionDiff {
                    subscribe: ["different/topic".into(), "some/#".into()].into(),
                    unsubscribe: ["different/+".into(), "some/topic".into()].into(),
                }
            );
        }

        #[test]
        fn merging_a_subscribe_with_a_matching_unsubscribe_cancels_out() {
            let mut diff = SubscriptionDiff {
                subscribe: ["some/topic".into()].into(),
                unsubscribe: [].into(),
            };
            let rhs = SubscriptionDiff {
                subscribe: [].into(),
                unsubscribe: ["some/topic".into()].into(),
            };
            diff += rhs.clone();

            assert_eq!(diff, SubscriptionDiff::empty());
        }

        #[test]
        fn merging_an_usubscribe_with_a_matching_subscribe_cancels_out() {
            let mut diff = SubscriptionDiff {
                subscribe: [].into(),
                unsubscribe: ["some/topic".into()].into(),
            };
            let rhs = SubscriptionDiff {
                subscribe: ["some/topic".into()].into(),
                unsubscribe: [].into(),
            };
            diff += rhs.clone();

            assert_eq!(diff, SubscriptionDiff::empty());
        }
    }

    mod rank_topic {
        use super::*;
        use std::cmp::Ordering;

        #[test]
        fn single_segment_wildcard_ranks_higher_than_static_topic() {
            assert_eq!(
                RankTopicFilter("a/+/c").partial_cmp(&RankTopicFilter("a/b/c")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn static_topic_ranks_lower_than_single_wildcard() {
            assert_eq!(
                RankTopicFilter("a/b/c").partial_cmp(&RankTopicFilter("a/+/c")),
                Some(Ordering::Less)
            );
        }

        #[test]
        fn static_topic_ranks_lower_than_global_wildcard() {
            assert_eq!(
                RankTopicFilter("a/b/c").partial_cmp(&RankTopicFilter("a/#")),
                Some(Ordering::Less)
            );
        }

        #[test]
        fn global_wildcard_ranks_higher_than_segment_wildcard() {
            assert_eq!(
                RankTopicFilter("a/#").partial_cmp(&RankTopicFilter("a/+")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn matching_static_topics_rank_equally() {
            assert_eq!(
                RankTopicFilter("a/b/c").partial_cmp(&RankTopicFilter("a/b/c")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn matching_global_wildcard_topics_rank_equally() {
            assert_eq!(
                RankTopicFilter("a/b/#").partial_cmp(&RankTopicFilter("a/b/#")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn matching_segment_wildcard_topics_rank_equally() {
            assert_eq!(
                RankTopicFilter("a/b/+").partial_cmp(&RankTopicFilter("a/b/+")),
                Some(Ordering::Equal)
            );
        }

        #[test]
        fn disjoint_static_topics_do_not_compare() {
            assert_eq!(
                RankTopicFilter("a/b").partial_cmp(&RankTopicFilter("b/c")),
                None
            );
        }

        #[test]
        fn partially_disjoint_static_topics_do_not_compare() {
            assert_eq!(
                RankTopicFilter("a/b/c").partial_cmp(&RankTopicFilter("a/b/d")),
                None
            );
        }

        // (Some(_), Some("+"), Some(Winner::This)) => break None
        #[test]
        fn topics_with_disjoint_wildcards_do_not_compare_bis() {
            assert_eq!(
                RankTopicFilter("+/a").partial_cmp(&RankTopicFilter("a/+")),
                None
            )
        }

        #[test]
        fn topic_with_more_wildcards_ranks_higher() {
            assert_eq!(
                RankTopicFilter("a/+/+/d").partial_cmp(&RankTopicFilter("a/+/c/d")),
                Some(Ordering::Greater)
            );
        }

        #[test]
        fn topics_with_disjoint_wildcards_do_not_compare() {
            assert_eq!(
                RankTopicFilter("a/b/+/d").partial_cmp(&RankTopicFilter("a/+/c/d")),
                None
            );
        }

        // (Some("#"), None, _) => break Some(Ordering::Greater),
        #[test]
        fn global_wildcard_suffix_ranks_higher_than_parent_topic() {
            assert_eq!(
                RankTopicFilter("a/#").partial_cmp(&RankTopicFilter("a")),
                Some(Ordering::Greater)
            )
        }

        // (None, Some("#"), _) => break Some(Ordering::Less),
        #[test]
        fn parent_topic_ranks_lower_than_its_global_wildcard_suffix() {
            assert_eq!(
                RankTopicFilter("a").partial_cmp(&RankTopicFilter("a/#")),
                Some(Ordering::Less)
            );
        }

        //(None, Some("#"), Some(Winner::This)) => break None,
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix() {
            assert_eq!(
                RankTopicFilter("+").partial_cmp(&RankTopicFilter("a/#")),
                None
            );
        }

        // (Some("#"), None, Some(Winner::Other)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_bis() {
            assert_eq!(
                RankTopicFilter("a/#").partial_cmp(&RankTopicFilter("+")),
                None
            );
        }

        // (Some(_), Some("#"), Some(Winner::This)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_ter() {
            assert_eq!(
                RankTopicFilter("+/a").partial_cmp(&RankTopicFilter("a/#")),
                None
            );
        }

        // (Some("#"), Some(_), Some(Winner::Other)) => break None
        #[test]
        fn global_wildcard_does_not_compare_with_larger_prefix_4() {
            assert_eq!(
                RankTopicFilter("a/#").partial_cmp(&RankTopicFilter("+/a")),
                None
            );
        }

        // (Some(_), None, _) => break None
        #[test]
        fn static_topics_of_different_lengths_do_not_compare() {
            assert_eq!(
                RankTopicFilter("a/b/c").partial_cmp(&RankTopicFilter("a/b")),
                None
            );
        }
    }

    mod prop_rank_topic {
        use super::*;
        use proptest::collection;
        use proptest::prop_compose;
        use proptest::proptest;
        use std::cmp::Ordering;

        prop_compose! {
            pub fn random_name()(id in "[abc]") -> String {
                id.to_string()
            }
        }

        prop_compose! {
            pub fn random_name_or_wildcard()(id in "[abc+]") -> String {
                id.to_string()
            }
        }

        prop_compose! {
            pub fn random_topic(max_length: usize)(
                vec in collection::vec(random_name(), 0..max_length)
            ) -> String
            {
                vec.join("/")
            }
        }

        prop_compose! {
            pub fn random_filter_not_global(max_length: usize)(
                vec in collection::vec(random_name_or_wildcard(), 0..max_length)
            ) -> String
            {
                vec.join("/")
            }
        }

        prop_compose! {
            pub fn random_filter(max_length: usize)(
                base in random_filter_not_global(max_length),
                global in proptest::bool::weighted(0.25),
            ) -> String
            {
                if global {
                    format!("{base}/#")
                } else {
                    base
                }
            }
        }

        prop_compose! {
            pub fn random_subscriptions(max_count: usize, max_length: usize)(
                vec in collection::vec(random_filter(max_length), 0..max_count)
            ) -> Vec<String>
            {
                vec
            }
        }

        pub fn matching_topic(filter: &str) -> String {
            let mut parts = filter.split('/').collect::<Vec<_>>();
            for part in parts.iter_mut() {
                *part = match *part {
                    "#" => "e/f",
                    "+" => "d",
                    s => s,
                };
            }
            parts.join("/")
        }

        fn add_wildcard(filter: &str, pos: usize) -> String {
            let mut parts = filter.split('/').collect::<Vec<_>>();
            if pos < parts.len() && parts[pos] != "#" {
                parts[pos] = "+"
            };
            parts.join("/")
        }

        proptest! {
            #[test]
            fn equality_is_reflexive(x in random_filter(5)) {
                assert_eq!(
                    RankTopicFilter(&x).partial_cmp(&RankTopicFilter(&x)),
                    Some(Ordering::Equal)
                );
            }

            #[test]
            fn rank_is_asymmetric(x in random_filter(5), y in random_filter(5)) {
                let x_cmp_y = RankTopicFilter(&x).partial_cmp(&RankTopicFilter(&y));
                let y_cmp_x = RankTopicFilter(&y).partial_cmp(&RankTopicFilter(&x));
                match (x_cmp_y, y_cmp_x) {
                    (Some(Ordering::Less), Some(Ordering::Greater)) => (),
                    (Some(Ordering::Greater), Some(Ordering::Less)) => (),
                    (Some(Ordering::Equal), Some(Ordering::Equal)) => (),
                    (None, None) => (),
                    (_,_) => panic!(),
                }
            }

            #[test]
            fn rank_is_transitive(x in random_filter(5), y in random_filter(5), z in random_filter(5)) {
                let x_cmp_y = RankTopicFilter(&x).partial_cmp(&RankTopicFilter(&y));
                let y_cmp_z = RankTopicFilter(&y).partial_cmp(&RankTopicFilter(&z));
                let x_cmp_z = RankTopicFilter(&x).partial_cmp(&RankTopicFilter(&z));
                match (x_cmp_y, y_cmp_z) {
                    (Some(Ordering::Less), Some(Ordering::Less))
                    | (Some(Ordering::Equal), Some(Ordering::Less))
                    | (Some(Ordering::Less), Some(Ordering::Equal))
                    => assert_eq!(x_cmp_z, Some(Ordering::Less)),

                    (Some(Ordering::Greater), Some(Ordering::Greater))
                    | (Some(Ordering::Equal), Some(Ordering::Greater))
                    | (Some(Ordering::Greater), Some(Ordering::Equal))
                    => assert_eq!(x_cmp_z, Some(Ordering::Greater)),

                    (_, _) => (),
                }
            }

            #[test]
            fn wildcard_is_greater_than_base(base in random_filter(5), i in 0..5) {
                let filter = add_wildcard(&base, i as usize);
                let cmp = RankTopicFilter(&filter).partial_cmp(&RankTopicFilter(&base));
                if cmp != Some(Ordering::Equal) {
                    assert_eq!(cmp, Some(Ordering::Greater));
                }
            }

            #[test]
            fn global_wildcard_is_greater_than_base(base in random_filter_not_global(5)) {
                let global = format!("{base}/#");
                assert_eq!(
                    RankTopicFilter(&global).partial_cmp(&RankTopicFilter(&base)),
                    Some(Ordering::Greater)
                );
            }

            #[test]
            fn global_wildcard_is_greater_than_any_suffix(base in random_filter_not_global(5), suffix in random_filter(5)) {
                let global = format!("{base}/#");
                let suffix = format!("{base}/{suffix}");
                assert_eq!(
                    RankTopicFilter(&global).partial_cmp(&RankTopicFilter(&suffix)),
                    Some(Ordering::Greater)
                );
            }

            #[test]
            fn mqttrie_matches_all_its_subscriptions(subscriptions in random_subscriptions(10, 5)) {
                let mut t = MqtTrie::default();
                for s in &subscriptions {
                    t.insert(s,1);
                }
                for topic in subscriptions.iter().map(|s| matching_topic(s)) {
                    assert!(!t.matches(&topic).is_empty());
                }
            }

            #[test]
            fn removing_all_subscriptions_lead_to_an_empty_mqttrie(subscriptions in random_subscriptions(10, 5)) {
                let mut t = MqtTrie::default();
                for s in &subscriptions {
                    t.insert(s,1);
                }
                for s in &subscriptions {
                    t.remove(s,&1);
                }
                assert!(t.is_empty())
            }

            #[test]
            fn mqttrie_extend_is_growing_when_adding_subscriptions(subscriptions in random_subscriptions(10, 5)) {
                let mut t = MqtTrie::default();
                for s in subscriptions {
                    let diff = t.insert(&s,1);
                    match diff.subscribe.iter().collect::<Vec<_>>()[..] {
                        [] => (),
                        [added] => assert_eq!(added, &s, "Insert({s}) subscribed to a different topic: {added}"),
                        _ => panic!("Insert subscribed more than one topic at once: {diff:?}")
                    }
                    for removed in &diff.unsubscribe {
                        assert!(
                            RankTopicFilter(&s) > RankTopicFilter(removed),
                            "Subscribing to {s} unsubscribed to topic that isn't smaller ({removed}): {diff:?}");
                    }
                }
            }

            #[test]
            fn mqttrie_extend_is_decreasing_when_removing_subscriptions(
                subscriptions in random_subscriptions(10, 5))
            {
                let mut t = MqtTrie::default();
                for s in &subscriptions {
                    t.insert(s,1);
                }
                for s in subscriptions {
                    let diff = t.remove(&s,&1);
                    match diff.unsubscribe.iter().collect::<Vec<_>>()[..] {
                        [] => (),
                        [removed] => assert_eq!(removed, &s, "Remove({s}) unsubscribed to a different topic: {removed}"),
                        _ => panic!("Remove unsubscribed more than one topic at once: {diff:?}")
                    }
                    for added in &diff.subscribe {
                        assert!(
                            RankTopicFilter(&s) > RankTopicFilter(added),
                            "Unsubscribing from {s} resubscribed to topic that isn't smaller ({added}): {diff:?}");
                    }
                }
            }

            #[test]
            fn removing_subscriptions_only_ever_unsubscribes_the_target_topic(
                subscriptions in random_subscriptions(10, 5))
            {
                let mut t = MqtTrie::default();
                for s in &subscriptions {
                    t.insert(s,1);
                }
                for s in subscriptions {
                    let diff = t.remove(&s,&1);
                    assert!(diff.unsubscribe.len() <= 1, "Diff contained more than one unsubscribe when removing {s}: {diff:?}");
                    if !diff.unsubscribe.is_empty() {
                        assert_eq!(diff.unsubscribe, [s.clone()].into(), "Diff contained unsubscribe for non-removed topic")
                    }
                }
            }

            #[test]
            fn cumulated_diff_over_added_subscriptions_should_contains_no_unsubscriptions(
                subscriptions in random_subscriptions(10, 5)
            ) {
                let mut diff = SubscriptionDiff::empty();
                let mut t = MqtTrie::default();
                for s in &subscriptions {
                    let prev_diff = diff.clone();
                    diff += t.insert(s,1);
                    assert!(diff.unsubscribe.is_empty(), "Inserting {s} (prev_diff={prev_diff:?}, diff={diff:?})");
                }
            }
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
                SubscriptionDiff {
                    unsubscribe: ["a/b/c".into(), "a/+".into()].into(),
                    subscribe: ["a/#".into()].into(),
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
                SubscriptionDiff {
                    subscribe: ["a/+/c/d".into()].into(),
                    unsubscribe: ["a/b/c/d".into()].into(),
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
                SubscriptionDiff {
                    subscribe: ["a/+/+/d".into()].into(),
                    unsubscribe: ["a/+/c/d".into()].into(),
                }
            );
        }

        #[test]
        fn does_not_subscribe_when_a_wildcard_is_already_superseded() {
            let mut t = MqtTrie::default();
            t.insert("a/+/+/d", 1);

            assert_eq!(t.insert("a/b/+/d", 2), SubscriptionDiff::empty());
            let mut t = MqtTrie::default();
            t.insert("a/b/+/+", 1);

            assert_eq!(t.insert("a/b/+/d", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn unsubscribes_only_to_subscribed_topics() {
            let mut t = MqtTrie::default();
            t.insert("a", 1);
            t.insert("a/+", 1);
            t.insert("a/b", 1);

            assert_eq!(
                t.insert("#", 1),
                SubscriptionDiff {
                    subscribe: ["#".into()].into(),
                    unsubscribe: ["a".into(), "a/+".into()].into(),
                }
            );
        }

        #[test]
        fn subscribing_to_global_wildcard_unsubscribes_parent_topic() {
            let mut t = MqtTrie::default();
            t.insert("a", 1);

            assert_eq!(
                t.insert("a/#", 2),
                SubscriptionDiff {
                    subscribe: ["a/#".into()].into(),
                    unsubscribe: ["a".into()].into(),
                }
            );
        }

        #[test]
        fn does_not_subscribe_parent_of_existing_global_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);

            assert_eq!(t.insert("a", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn does_not_unsubscribe_parent_of_existing_global_wildcard() {
            let mut t = MqtTrie::default();
            t.insert("a", 0);
            t.insert("a/#", 1);

            assert_eq!(t.insert("a/#", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn subscribing_to_multi_level_segment_wildcard_unsubscribes_matching_static_topic() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c/d", 1);

            assert_eq!(
                t.insert("a/+/+/+", 2),
                SubscriptionDiff {
                    subscribe: ["a/+/+/+".into()].into(),
                    unsubscribe: ["a/b/c/d".into()].into(),
                }
            );
        }

        #[test]
        fn resubscribing_to_the_mid_level_segment_wildcard_produces_an_empty_diff() {
            let mut t = MqtTrie::default();

            assert_eq!(t.insert("a/+/c", 1), SubscribeTo("a/+/c"));
            assert_eq!(t.insert("a/+/c", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn does_not_subscribe_sibling_of_top_level_global_wildcard() {
            let mut t = MqtTrie::default();

            t.insert("#", 0);
            assert_eq!(t.insert("a", 1), SubscriptionDiff::empty());
        }

        #[test]
        fn does_not_subscribe_sibling_of_top_level_segment_wildcard() {
            let mut t = MqtTrie::default();

            t.insert("+", 0);
            assert_eq!(t.insert("a", 1), SubscriptionDiff::empty());
        }

        #[test]
        fn subscribes_sibling_of_non_subscribed_top_level_segment_wildcard() {
            let mut t = MqtTrie::default();

            t.insert("+/b", 0);
            assert_eq!(t.insert("a", 1), SubscribeTo("a"));
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
                SubscriptionDiff {
                    subscribe: [].into(),
                    unsubscribe: ["a/b".into()].into(),
                }
            )
        }

        #[test]
        fn removing_one_of_multiple_subscribers_does_not_unsubscribe() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);
            t.insert("a/b", 2);

            assert_eq!(t.remove("a/b", &1), SubscriptionDiff::empty())
        }

        #[test]
        fn removing_wildcard_topic_unsubscribes() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c/#", 1);

            assert_eq!(
                t.remove("a/+/c/#", &1),
                SubscriptionDiff {
                    unsubscribe: ["a/+/c/#".into()].into(),
                    subscribe: [].into(),
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
                SubscriptionDiff {
                    unsubscribe: ["a/+/c".into()].into(),
                    subscribe: ["a/b/c".into()].into(),
                }
            );
        }

        #[test]
        fn removing_wildcard_topic_resubscribes_only_to_required_topics() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c/d", 1);
            t.insert("a/+/+/+", 2);
            t.insert("a/b/+/d", 3);

            assert_eq!(
                t.remove("a/+/+/+", &2),
                SubscriptionDiff {
                    unsubscribe: ["a/+/+/+".into()].into(),
                    subscribe: ["a/b/+/d".into()].into(),
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
                SubscriptionDiff {
                    unsubscribe: ["a/#".into()].into(),
                    subscribe: ["a/b/c".into()].into(),
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
                SubscriptionDiff {
                    unsubscribe: ["a/#".into()].into(),
                    subscribe: ["a".into()].into(),
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
                SubscriptionDiff {
                    unsubscribe: ["a/+/+/d".into()].into(),
                    subscribe: ["a/+/c/d".into()].into(),
                }
            );
        }

        #[test]
        fn unsubscribing_from_topic_masked_by_global_wildcard_subscription_changes_nothing() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 1);
            t.insert("a/b/c", 2);

            assert_eq!(t.remove("a/b/c", &2), SubscriptionDiff::empty());
        }

        #[test]
        fn unsubscribing_from_a_non_subscribed_topic_changes_nothing() {
            let mut t = MqtTrie::default();

            assert_eq!(t.remove("a/b/c", &1), SubscriptionDiff::empty());
        }

        #[test]
        fn unsubscribing_from_an_end_segment_wildcard_resubscribes_to_existing_topics() {
            let mut t = MqtTrie::default();
            t.insert("a/b", 1);
            t.insert("a/+", 2);

            assert_eq!(
                t.remove("a/+", &2),
                SubscriptionDiff {
                    unsubscribe: ["a/+".into()].into(),
                    subscribe: ["a/b".into()].into(),
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
                SubscriptionDiff {
                    unsubscribe: ["a/+".into()].into(),
                    subscribe: [].into(),
                }
            );
        }

        #[test]
        fn unsubscribing_from_static_parent_does_not_unsubscribe_child() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);
            t.insert("a/b", 2);

            assert_eq!(
                t.remove("a/b", &2),
                SubscriptionDiff {
                    unsubscribe: ["a/b".into()].into(),
                    subscribe: [].into(),
                }
            );
            assert_eq!(t.matches("a/b/c"), [&1]);
        }

        #[test]
        fn unsubscribing_from_a_non_subscribed_topic_produces_an_empty_diff() {
            let mut t = MqtTrie::default();
            t.insert("a/b/c", 1);

            assert_eq!(
                t.remove("a/b/d", &1),
                SubscriptionDiff {
                    unsubscribe: [].into(),
                    subscribe: [].into(),
                }
            );
        }

        #[test]
        fn unsubscribing_resubscribes_to_matching_static_topic_if_wildcard_does_not_match() {
            let mut t = MqtTrie::default();
            t.insert("a/+/c/d", 0);
            t.insert("a/b/+/e", 1);
            t.insert("a/b/c/d", 2);

            assert_eq!(
                t.remove("a/+/c/d", &0),
                SubscriptionDiff {
                    subscribe: ["a/b/c/d".into()].into(),
                    unsubscribe: ["a/+/c/d".into()].into()
                }
            );
        }

        #[test]
        fn testing() {
            let mut t = MqtTrie::default();
            t.insert("+/+/a/+", 1);
            assert_eq!(t.insert("a/#", 1), SubscribeTo("a/#"));
            assert_eq!(
                t.remove("+/+/a/+", &1),
                SubscriptionDiff {
                    unsubscribe: ["+/+/a/+".into()].into(),
                    subscribe: [].into()
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

    mod remove_unneeded_topics {
        use super::*;

        #[test]
        fn cover_all_branches() {
            // This is tested elsewhere, but non-deterministically
            assert_eq!(
                remove_unneeded_topics(&["a/+", "#", "a/b"]),
                ["#".into()].into()
            )
        }
    }

    mod cases {
        use super::*;

        #[test]
        fn c8y_mapper() {
            let topics = [
                "c8y-internal/alarms/+/+/+/+/+/a/+",
                "c8y/s/ds",
                "c8y/devicecontrol/notifications",
                "te/+/+/+/+/cmd/+/+",
                "te/+/+/+/+/cmd/+",
                "te/+/+/+/+",
                "te/+/+/+/+/twin/+",
                "te/+/+/+/+/m/+",
                "te/+/+/+/+/e/+",
                "te/+/+/+/+/a/+",
                "te/+/+/+/+/status/health",
            ];

            let mut t = MqtTrie::default();
            let mut diff = SubscriptionDiff::empty();
            for topic in topics {
                diff += t.insert(topic, 0);
            }
            assert_eq!(
                diff,
                SubscriptionDiff {
                    subscribe: topics.into_iter().map(<_>::to_owned).collect(),
                    unsubscribe: <_>::default(),
                }
            );
            t.insert("te/#", 1);

            assert_eq!(
                t.matches("te/device/child-007/service/tedge-agent"),
                [&0, &1]
            );
        }

        #[test]
        fn tedge_agent() {
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]: [crates/extensions/tedge_mqtt_ext/src/lib.rs:172:66] topic = "te/+/+/+/+/cmd/config_update/+"
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]: [crates/extensions/tedge_mqtt_ext/src/lib.rs:172:39] self.trie.trie.insert(dbg!(topic), client_id) = SubscriptionDiff {
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]:     subscribe: {},
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]:     unsubscribe: {
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]:         "te/+/+/+/+/#",
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]:         "te/device/main///cmd/+/+",
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]:     },
            // Jun 16 16:14:13 james-Precision-3591 tedge-agent[357845]: }
            let mut t = MqtTrie::default();
            t.insert("te/+/+/+/+/#", 0);
            t.insert("te/device/main///cmd/+/+", 1);
            assert_eq!(
                t.insert("te/+/+/+/+/cmd/config_update/+", 2),
                SubscriptionDiff::empty()
            );
        }

        #[test]
        fn issue_1() {
            let mut t = MqtTrie::default();
            t.insert("/#", 0);
            assert!(t.insert("+/+", 1).unsubscribe.is_empty(),);
        }

        #[test]
        fn issue_2() {
            let mut t = MqtTrie::default();
            t.insert("+/a/a", 0);
            t.insert("/#", 0);
            assert!(t.remove("+/a/a", &0).subscribe.is_empty());
            assert!(t.remove("/#", &0).subscribe.is_empty());
        }

        #[test]
        fn issue_3() {
            let mut t = MqtTrie::default();
            t.insert("a/+/+/a", 0);

            assert_eq!(t.insert("+/a/+/+", 0), SubscribeTo("+/a/+/+"));
        }

        #[test]
        fn issue_4() {
            let mut t = MqtTrie::default();
            t.insert("a", 0);
            t.insert("+", 0);

            assert_eq!(t.insert("a/#", 0), SubscribeTo("a/#"));
        }

        #[test]
        fn issue_5() {
            let mut t = MqtTrie::default();
            t.insert("a/#", 0);
            t.insert("a", 0);

            assert_eq!(t.insert("+", 0), SubscribeTo("+"));
        }

        #[test]
        fn issue_6() {
            let mut t = MqtTrie::default();
            t.insert("a/b/#", 0);
            t.insert("a/b", 1);

            assert_eq!(t.insert("a/+", 2), SubscribeTo("a/+"));
        }

        #[test]
        fn issue_7() {
            let mut t = MqtTrie::default();
            t.insert("c/a", 0);
            t.insert("+/+", 1);

            assert_eq!(t.insert("c/+", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn issue_8() {
            let mut t = MqtTrie::default();
            t.insert("b/a", 0);
            t.insert("+/#", 1);

            assert_eq!(t.insert("b/+", 2), SubscriptionDiff::empty());
        }

        #[test]
        fn issue_9() {
            let mut t = MqtTrie::default();
            t.insert("+/+/a/#", 0);
            t.insert("+/+/#", 1);

            assert_eq!(
                t.insert("+/#", 2),
                SubscriptionDiff {
                    subscribe: ["+/#".into()].into(),
                    unsubscribe: ["+/+/#".into()].into(),
                }
            );
        }

        #[test]
        fn issue_10() {
            let mut t = MqtTrie::default();
            t.insert("+/a/#", 0);
            t.insert("b/a/+", 1);

            // b/a/+ is superseded by +/a/#
            assert_eq!(
                t.insert("b/+/+", 2),
                SubscriptionDiff {
                    subscribe: ["b/+/+".into()].into(),
                    unsubscribe: [].into(),
                }
            );
        }

        #[test]
        fn issue_11() {
            let mut t = MqtTrie::default();
            t.insert("c/+", 0);
            t.insert("c/c", 1);

            assert_eq!(
                t.insert("+/c", 2),
                SubscriptionDiff {
                    subscribe: ["+/c".into()].into(),
                    unsubscribe: [].into(),
                }
            );
        }

        #[test]
        fn issue_12() {
            let mut t = MqtTrie::default();
            t.insert("a/+", 0);
            t.insert("+/+", 1);
            t.insert("+/+/#", 2);

            assert_eq!(t.insert("+/+", 3), SubscriptionDiff::empty());
        }

        #[test]
        fn issue_13() {
            let mut t = MqtTrie::default();
            t.insert("+/c/a/+", 0);
            t.insert("b/c/a/c", 1);

            // b/c/a/c < +/c/a/+ so isn't currently subscribed
            assert_eq!(
                t.insert("b/+/+/c", 2),
                SubscriptionDiff {
                    subscribe: ["b/+/+/c".into()].into(),
                    unsubscribe: [].into(),
                }
            );
        }
    }
}
