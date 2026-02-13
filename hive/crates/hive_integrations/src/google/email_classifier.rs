//! Email classification engine.
//!
//! Implements a 5-layer classification system that categorises emails by
//! analysing Gmail labels, sender address, subject line patterns, body
//! content, and a combined confidence score.

use regex::RegexSet;
use serde::{Deserialize, Serialize};

// ── Types ─────────────────────────────────────────────────────────

/// High-level category assigned to an email.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmailCategory {
    Important,
    Personal,
    Work,
    Newsletter,
    Marketing,
    Spam,
    Transactional,
    Social,
}

/// Result of running the classifier on a single email.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub category: EmailCategory,
    pub confidence: f32,
    pub is_newsletter: bool,
    pub is_marketing: bool,
    pub reasons: Vec<String>,
}

// ── Classifier ────────────────────────────────────────────────────

/// Five-layer email classifier.
///
/// Layers are evaluated in order and their scores are combined:
///
/// 1. **Label-based** -- Gmail labels (CATEGORY_PROMOTIONS, etc.)
/// 2. **Sender analysis** -- known newsletter / marketing domains
/// 3. **Subject line patterns** -- keywords such as *unsubscribe*, *deal*, *sale*
/// 4. **Body analysis** -- unsubscribe links, tracking pixel patterns
/// 5. **Combined score** -- weighted aggregation of all layers
pub struct EmailClassifier {
    // Pre-compiled regex sets for each category (built once in `new`).
    newsletter_re: RegexSet,
    marketing_re: RegexSet,
    spam_re: RegexSet,
}

impl EmailClassifier {
    /// Create a new classifier pre-loaded with built-in pattern lists.
    pub fn new() -> Self {
        let newsletter_patterns: Vec<String> = vec![
            "newsletter".into(),
            "digest".into(),
            "weekly roundup".into(),
            "daily briefing".into(),
            "weekly update".into(),
            "monthly update".into(),
            "substack".into(),
            "mailchimp".into(),
            "convertkit".into(),
            "buttondown".into(),
            "revue".into(),
            "beehiiv".into(),
            "list-unsubscribe".into(),
        ];

        let marketing_patterns: Vec<String> = vec![
            "% off".into(),
            "discount".into(),
            "limited time".into(),
            "act now".into(),
            "special offer".into(),
            "deal of the day".into(),
            "free shipping".into(),
            "buy now".into(),
            "shop now".into(),
            "exclusive offer".into(),
            "promo code".into(),
            "sale ends".into(),
            "flash sale".into(),
            "clearance".into(),
            "coupon".into(),
        ];

        let spam_patterns: Vec<String> = vec![
            "you have won".into(),
            "congratulations".into(),
            "click here immediately".into(),
            "act now or".into(),
            "million dollars".into(),
            "nigerian prince".into(),
            "wire transfer".into(),
            "bitcoin opportunity".into(),
            "double your".into(),
            "guaranteed income".into(),
            "risk.free".into(),
            "no obligation".into(),
        ];

        // Build regex sets (case-insensitive).
        let to_re = |patterns: &[String]| {
            let escaped: Vec<String> = patterns
                .iter()
                .map(|p| format!("(?i){}", regex::escape(p)))
                .collect();
            RegexSet::new(&escaped).expect("invalid regex patterns")
        };

        let newsletter_re = to_re(&newsletter_patterns);
        let marketing_re = to_re(&marketing_patterns);
        let spam_re = to_re(&spam_patterns);

        Self {
            newsletter_re,
            marketing_re,
            spam_re,
        }
    }

    // ── Public API ────────────────────────────────────────────────

    /// Classify a single email.
    pub fn classify(
        &self,
        from: &str,
        subject: &str,
        body: &str,
        labels: &[String],
    ) -> ClassificationResult {
        let mut scores: Vec<(EmailCategory, f32, String)> = Vec::new();

        // Layer 1 -- Gmail labels
        self.score_labels(labels, &mut scores);

        // Layer 2 -- Sender analysis
        self.score_sender(from, &mut scores);

        // Layer 3 -- Subject patterns
        self.score_subject(subject, &mut scores);

        // Layer 4 -- Body analysis
        self.score_body(body, &mut scores);

        // Layer 5 -- Combine
        self.combine(scores)
    }

    /// Check whether an email looks like a newsletter.
    pub fn is_newsletter(&self, from: &str, subject: &str, body: &str) -> bool {
        let combined = format!("{from} {subject} {body}");
        self.newsletter_re.is_match(&combined)
    }

    /// Check whether an email looks like marketing.
    pub fn is_marketing(&self, from: &str, subject: &str, body: &str) -> bool {
        let combined = format!("{from} {subject} {body}");
        self.marketing_re.is_match(&combined)
    }

    /// Classify a batch of emails.
    pub fn classify_batch(
        &self,
        emails: &[(&str, &str, &str, &[String])],
    ) -> Vec<ClassificationResult> {
        emails
            .iter()
            .map(|(from, subject, body, labels)| self.classify(from, subject, body, labels))
            .collect()
    }

    // ── Layer 1: Label-based scoring ──────────────────────────────

    fn score_labels(&self, labels: &[String], scores: &mut Vec<(EmailCategory, f32, String)>) {
        for label in labels {
            let upper = label.to_uppercase();
            match upper.as_str() {
                "IMPORTANT" | "STARRED" => {
                    scores.push((
                        EmailCategory::Important,
                        0.8,
                        format!("Gmail label: {label}"),
                    ));
                }
                "CATEGORY_PERSONAL" => {
                    scores.push((
                        EmailCategory::Personal,
                        0.7,
                        "Gmail label: CATEGORY_PERSONAL".into(),
                    ));
                }
                "CATEGORY_PROMOTIONS" => {
                    scores.push((
                        EmailCategory::Marketing,
                        0.7,
                        "Gmail label: CATEGORY_PROMOTIONS".into(),
                    ));
                }
                "CATEGORY_SOCIAL" => {
                    scores.push((
                        EmailCategory::Social,
                        0.7,
                        "Gmail label: CATEGORY_SOCIAL".into(),
                    ));
                }
                "CATEGORY_UPDATES" => {
                    scores.push((
                        EmailCategory::Transactional,
                        0.5,
                        "Gmail label: CATEGORY_UPDATES".into(),
                    ));
                }
                "CATEGORY_FORUMS" => {
                    scores.push((
                        EmailCategory::Social,
                        0.5,
                        "Gmail label: CATEGORY_FORUMS".into(),
                    ));
                }
                "SPAM" => {
                    scores.push((EmailCategory::Spam, 0.9, "Gmail label: SPAM".into()));
                }
                _ => {}
            }
        }
    }

    // ── Layer 2: Sender analysis ──────────────────────────────────

    fn score_sender(&self, from: &str, scores: &mut Vec<(EmailCategory, f32, String)>) {
        let lower = from.to_lowercase();

        // Known newsletter senders / domains
        let newsletter_domains = [
            "substack.com",
            "mail.beehiiv.com",
            "convertkit.com",
            "buttondown.email",
            "revue.email",
            "mailchimp.com",
            "list-manage.com",
            "sendinblue.com",
            "mailgun.org",
        ];
        for domain in &newsletter_domains {
            if lower.contains(domain) {
                scores.push((
                    EmailCategory::Newsletter,
                    0.7,
                    format!("sender domain: {domain}"),
                ));
                return;
            }
        }

        // Marketing senders
        let marketing_senders = [
            "promo@",
            "deals@",
            "offers@",
            "marketing@",
            "sales@",
            "promotions@",
            "shop@",
            "store@",
        ];
        for prefix in &marketing_senders {
            if lower.contains(prefix) {
                scores.push((
                    EmailCategory::Marketing,
                    0.6,
                    format!("sender prefix: {prefix}"),
                ));
                return;
            }
        }

        // Transactional senders
        let transactional = [
            "noreply@",
            "no-reply@",
            "notifications@",
            "alert@",
            "alerts@",
            "receipts@",
            "billing@",
            "orders@",
            "shipping@",
            "support@",
        ];
        for prefix in &transactional {
            if lower.contains(prefix) {
                scores.push((
                    EmailCategory::Transactional,
                    0.5,
                    format!("sender prefix: {prefix}"),
                ));
                return;
            }
        }

        // Social senders
        let social_domains = [
            "facebookmail.com",
            "linkedin.com",
            "twitter.com",
            "instagram.com",
            "plus.google.com",
            "pinterest.com",
            "tiktok.com",
        ];
        for domain in &social_domains {
            if lower.contains(domain) {
                scores.push((
                    EmailCategory::Social,
                    0.6,
                    format!("social domain: {domain}"),
                ));
                return;
            }
        }
    }

    // ── Layer 3: Subject patterns ─────────────────────────────────

    fn score_subject(&self, subject: &str, scores: &mut Vec<(EmailCategory, f32, String)>) {
        if self.newsletter_re.is_match(subject) {
            scores.push((
                EmailCategory::Newsletter,
                0.5,
                "subject matches newsletter pattern".into(),
            ));
        }
        if self.marketing_re.is_match(subject) {
            scores.push((
                EmailCategory::Marketing,
                0.5,
                "subject matches marketing pattern".into(),
            ));
        }
        if self.spam_re.is_match(subject) {
            scores.push((
                EmailCategory::Spam,
                0.6,
                "subject matches spam pattern".into(),
            ));
        }

        // Transactional subject keywords
        let transactional_kw = [
            "order confirmation",
            "shipping confirmation",
            "receipt",
            "invoice",
            "password reset",
            "verify your",
            "your account",
            "payment received",
            "booking confirmation",
        ];
        let lower = subject.to_lowercase();
        for kw in &transactional_kw {
            if lower.contains(kw) {
                scores.push((
                    EmailCategory::Transactional,
                    0.6,
                    format!("subject keyword: {kw}"),
                ));
                break;
            }
        }
    }

    // ── Layer 4: Body analysis ────────────────────────────────────

    fn score_body(&self, body: &str, scores: &mut Vec<(EmailCategory, f32, String)>) {
        let lower = body.to_lowercase();

        // Unsubscribe link -- strong newsletter / marketing signal
        if lower.contains("unsubscribe") || lower.contains("list-unsubscribe") {
            scores.push((
                EmailCategory::Newsletter,
                0.4,
                "body contains unsubscribe link".into(),
            ));
        }

        // Tracking pixel patterns
        let pixel_patterns = [
            "tracking pixel",
            "open.gif",
            "pixel.gif",
            "1x1",
            "beacon.gif",
            "track/open",
        ];
        for pat in &pixel_patterns {
            if lower.contains(pat) {
                scores.push((
                    EmailCategory::Marketing,
                    0.3,
                    format!("body tracking pattern: {pat}"),
                ));
                break;
            }
        }

        // Spam body patterns
        if self.spam_re.is_match(body) {
            scores.push((EmailCategory::Spam, 0.5, "body matches spam pattern".into()));
        }

        // Marketing body patterns
        if self.marketing_re.is_match(body) {
            scores.push((
                EmailCategory::Marketing,
                0.3,
                "body matches marketing pattern".into(),
            ));
        }
    }

    // ── Layer 5: Combine scores ───────────────────────────────────

    fn combine(&self, scores: Vec<(EmailCategory, f32, String)>) -> ClassificationResult {
        if scores.is_empty() {
            return ClassificationResult {
                category: EmailCategory::Personal,
                confidence: 0.3,
                is_newsletter: false,
                is_marketing: false,
                reasons: vec!["no strong signals detected; defaulting to Personal".into()],
            };
        }

        // Aggregate per-category confidence (take max per category).
        let categories = [
            EmailCategory::Important,
            EmailCategory::Personal,
            EmailCategory::Work,
            EmailCategory::Newsletter,
            EmailCategory::Marketing,
            EmailCategory::Spam,
            EmailCategory::Transactional,
            EmailCategory::Social,
        ];

        let mut best_cat = EmailCategory::Personal;
        let mut best_conf: f32 = 0.0;
        let mut reasons: Vec<String> = Vec::new();
        let mut has_newsletter = false;
        let mut has_marketing = false;

        for cat in &categories {
            let matching: Vec<&(EmailCategory, f32, String)> =
                scores.iter().filter(|(c, _, _)| c == cat).collect();
            if matching.is_empty() {
                continue;
            }
            // Combined confidence: take the maximum single signal, plus a small
            // boost for each additional signal (capped at 1.0).
            let max_score = matching.iter().map(|(_, s, _)| *s).fold(0.0f32, f32::max);
            let bonus = (matching.len() as f32 - 1.0) * 0.1;
            let combined = (max_score + bonus).min(1.0);

            for (_, _, reason) in &matching {
                reasons.push(reason.clone());
            }

            if *cat == EmailCategory::Newsletter {
                has_newsletter = true;
            }
            if *cat == EmailCategory::Marketing {
                has_marketing = true;
            }

            if combined > best_conf {
                best_conf = combined;
                best_cat = *cat;
            }
        }

        ClassificationResult {
            category: best_cat,
            confidence: best_conf,
            is_newsletter: has_newsletter,
            is_marketing: has_marketing,
            reasons,
        }
    }
}

impl Default for EmailClassifier {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier() -> EmailClassifier {
        EmailClassifier::new()
    }

    // 1. Spam label gives Spam category
    #[test]
    fn test_classify_spam_label() {
        let c = classifier();
        let res = c.classify("spammer@bad.com", "You won!", "", &["SPAM".to_string()]);
        assert_eq!(res.category, EmailCategory::Spam);
        assert!(res.confidence >= 0.8);
    }

    // 2. Promotions label gives Marketing
    #[test]
    fn test_classify_promotions_label() {
        let c = classifier();
        let res = c.classify(
            "store@shop.com",
            "Big sale",
            "",
            &["CATEGORY_PROMOTIONS".to_string()],
        );
        assert_eq!(res.category, EmailCategory::Marketing);
    }

    // 3. Newsletter sender domain
    #[test]
    fn test_classify_newsletter_sender() {
        let c = classifier();
        let res = c.classify(
            "updates@substack.com",
            "Your weekly digest",
            "Read more at...",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Newsletter);
        assert!(res.is_newsletter);
    }

    // 4. Marketing subject patterns
    #[test]
    fn test_classify_marketing_subject() {
        let c = classifier();
        let res = c.classify(
            "shop@example.com",
            "50% off everything - limited time!",
            "",
            &[],
        );
        assert!(res.is_marketing);
    }

    // 5. Transactional email (order confirmation)
    #[test]
    fn test_classify_transactional() {
        let c = classifier();
        let res = c.classify(
            "noreply@amazon.com",
            "Order Confirmation #12345",
            "Your order has been placed.",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Transactional);
    }

    // 6. Social domain detection
    #[test]
    fn test_classify_social() {
        let c = classifier();
        let res = c.classify(
            "notification@facebookmail.com",
            "You have a new friend request",
            "",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Social);
    }

    // 7. Body unsubscribe link triggers newsletter flag
    #[test]
    fn test_body_unsubscribe_signal() {
        let c = classifier();
        let res = c.classify(
            "info@example.com",
            "Weekly digest",
            "Click here to unsubscribe from this newsletter",
            &[],
        );
        assert!(res.is_newsletter);
    }

    // 8. Default to Personal when no signals
    #[test]
    fn test_classify_default_personal() {
        let c = classifier();
        let res = c.classify(
            "friend@gmail.com",
            "Dinner tonight?",
            "Want to grab dinner at 7?",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Personal);
    }

    // 9. is_newsletter helper
    #[test]
    fn test_is_newsletter() {
        let c = classifier();
        assert!(c.is_newsletter("news@substack.com", "Weekly Newsletter", ""));
        assert!(!c.is_newsletter("friend@gmail.com", "Hey", "How are you?"));
    }

    // 10. is_marketing helper
    #[test]
    fn test_is_marketing() {
        let c = classifier();
        assert!(c.is_marketing("store@shop.com", "Flash sale - 50% off!", ""));
        assert!(!c.is_marketing("boss@work.com", "Q4 review", "Please review the doc."));
    }

    // 11. Batch classification
    #[test]
    fn test_classify_batch() {
        let c = classifier();
        let labels_empty: Vec<String> = vec![];
        let spam_labels = vec!["SPAM".to_string()];
        let emails: Vec<(&str, &str, &str, &[String])> = vec![
            (
                "friend@gmail.com",
                "Hey",
                "How are you?",
                labels_empty.as_slice(),
            ),
            ("spammer@bad.com", "Win now", "", spam_labels.as_slice()),
        ];
        let results = c.classify_batch(&emails);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].category, EmailCategory::Personal);
        assert_eq!(results[1].category, EmailCategory::Spam);
    }

    // 12. Important label
    #[test]
    fn test_classify_important_label() {
        let c = classifier();
        let res = c.classify(
            "boss@work.com",
            "Urgent: review needed",
            "",
            &["IMPORTANT".to_string()],
        );
        assert_eq!(res.category, EmailCategory::Important);
    }

    // 13. Multiple signals boost confidence
    #[test]
    fn test_multiple_signals_boost() {
        let c = classifier();
        // Newsletter sender + newsletter subject + unsubscribe body
        let res = c.classify(
            "digest@substack.com",
            "Your weekly newsletter digest",
            "Click here to unsubscribe",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Newsletter);
        assert!(res.confidence > 0.7);
    }

    // 14. Spam body patterns
    #[test]
    fn test_spam_body_detection() {
        let c = classifier();
        let res = c.classify(
            "unknown@mystery.com",
            "Important message",
            "You have won a million dollars! Click here immediately to claim.",
            &[],
        );
        assert_eq!(res.category, EmailCategory::Spam);
    }

    // 15. Tracking pixel in body triggers marketing
    #[test]
    fn test_tracking_pixel_detection() {
        let c = classifier();
        let res = c.classify(
            "deals@offers-store.com",
            "Check out our deals",
            "Special offer just for you <img src=\"track/open/pixel.gif\" width=\"1x1\">",
            &[],
        );
        assert!(res.is_marketing);
    }

    // 16. Serialization roundtrip
    #[test]
    fn test_classification_result_serialization() {
        let result = ClassificationResult {
            category: EmailCategory::Newsletter,
            confidence: 0.85,
            is_newsletter: true,
            is_marketing: false,
            reasons: vec!["test reason".into()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ClassificationResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category, EmailCategory::Newsletter);
        assert!((back.confidence - 0.85).abs() < f32::EPSILON);
        assert!(back.is_newsletter);
        assert!(!back.is_marketing);
    }

    // 17. EmailCategory serialization
    #[test]
    fn test_email_category_serialization() {
        let cat = EmailCategory::Transactional;
        let json = serde_json::to_string(&cat).unwrap();
        assert_eq!(json, "\"Transactional\"");
        let back: EmailCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(back, EmailCategory::Transactional);
    }
}
