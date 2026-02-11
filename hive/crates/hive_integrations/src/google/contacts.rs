//! Google Contacts (People API v1) client.
//!
//! Wraps the REST API at `https://people.googleapis.com/v1` using
//! `reqwest` for HTTP and bearer-token authentication.

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

const DEFAULT_BASE_URL: &str = "https://people.googleapis.com/v1";

/// A single contact from the Google People API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    /// Resource name, e.g. "people/c1234567890".
    #[serde(default)]
    pub resource_name: String,
    /// Primary display name assembled by the API.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Email addresses associated with the contact.
    #[serde(default)]
    pub email_addresses: Vec<EmailAddress>,
    /// Phone numbers associated with the contact.
    #[serde(default)]
    pub phone_numbers: Vec<PhoneNumber>,
    /// Organizations (company / job title) for the contact.
    #[serde(default)]
    pub organizations: Vec<Organization>,
}

/// An email address entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailAddress {
    pub value: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

/// A phone number entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneNumber {
    pub value: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

/// An organization (company / job title) entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// A paginated list of contacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactList {
    #[serde(default)]
    pub connections: Vec<Contact>,
    pub next_page_token: Option<String>,
    pub total_people: Option<u32>,
}

/// Search results wrapper returned by the People API `searchContacts` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    #[serde(default)]
    results: Vec<SearchResultEntry>,
}

/// A single search-result entry wrapping a [`Contact`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResultEntry {
    person: Contact,
}

/// Request body for creating or updating a contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateContactRequest {
    pub given_name: String,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// Client for the Google People API v1 (Contacts).
pub struct GoogleContactsClient {
    base_url: String,
    client: Client,
}

impl GoogleContactsClient {
    /// Create a new client using the given OAuth access token.
    pub fn new(access_token: &str) -> Self {
        Self::with_base_url(access_token, DEFAULT_BASE_URL)
    }

    /// Create a new client pointing at a custom base URL (useful for testing).
    pub fn with_base_url(access_token: &str, base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();

        let mut headers = HeaderMap::new();
        // access_token is assumed to be a valid bearer token
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {access_token}")) {
            headers.insert(AUTHORIZATION, val);
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { base_url, client }
    }

    /// Return the configured base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// List the authenticated user's contacts with pagination.
    pub async fn list_contacts(
        &self,
        page_size: u32,
        page_token: Option<&str>,
    ) -> Result<ContactList> {
        let mut url = format!(
            "{}/people/me/connections?pageSize={}\
             &personFields=names,emailAddresses,phoneNumbers,organizations",
            self.base_url, page_size
        );

        if let Some(token) = page_token {
            url.push_str(&format!("&pageToken={}", urlencod(token)));
        }

        debug!(url = %url, "listing contacts");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Contacts list_contacts request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse contact list")
    }

    /// Search contacts by a free-text query string.
    pub async fn search_contacts(
        &self,
        query: &str,
        page_size: u32,
    ) -> Result<Vec<Contact>> {
        let url = format!(
            "{}/people:searchContacts?query={}&pageSize={}\
             &readMask=names,emailAddresses,phoneNumbers,organizations",
            self.base_url,
            urlencod(query),
            page_size
        );

        debug!(url = %url, "searching contacts");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Contacts search_contacts request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        let result: SearchResult = resp
            .json()
            .await
            .context("failed to parse contact search results")?;

        Ok(result.results.into_iter().map(|r| r.person).collect())
    }

    /// Get a single contact by resource name (e.g. "people/c1234567890").
    pub async fn get_contact(&self, resource_name: &str) -> Result<Contact> {
        let url = format!(
            "{}/{}?personFields=names,emailAddresses,phoneNumbers,organizations",
            self.base_url, resource_name
        );

        debug!(url = %url, "getting contact");

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Contacts get_contact request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse contact")
    }

    /// Create a new contact.
    pub async fn create_contact(&self, request: &CreateContactRequest) -> Result<Contact> {
        let url = format!("{}/people:createContact", self.base_url);

        let body = build_contact_body(request);

        debug!(url = %url, "creating contact");

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Contacts create_contact request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse created contact")
    }

    /// Update an existing contact identified by `resource_name`.
    pub async fn update_contact(
        &self,
        resource_name: &str,
        request: &CreateContactRequest,
    ) -> Result<Contact> {
        let url = format!(
            "{}/{}:updateContact?updatePersonFields=names,emailAddresses,phoneNumbers,organizations",
            self.base_url, resource_name
        );

        let body = build_contact_body(request);

        debug!(url = %url, "updating contact");

        let resp = self
            .client
            .patch(&url)
            .json(&body)
            .send()
            .await
            .context("Contacts update_contact request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        resp.json().await.context("failed to parse updated contact")
    }

    /// Delete a contact by resource name.
    pub async fn delete_contact(&self, resource_name: &str) -> Result<()> {
        let url = format!("{}/{}:deleteContact", self.base_url, resource_name);

        debug!(url = %url, "deleting contact");

        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .context("Contacts delete_contact request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("People API error ({}): {}", status, body);
        }

        Ok(())
    }
}

/// Build the JSON body expected by the People API for create/update operations.
fn build_contact_body(req: &CreateContactRequest) -> serde_json::Value {
    let mut names = serde_json::json!([{
        "givenName": req.given_name,
    }]);
    if let Some(ref family) = req.family_name {
        names[0]["familyName"] = serde_json::Value::String(family.clone());
    }

    let mut body = serde_json::json!({ "names": names });

    if let Some(ref email) = req.email {
        body["emailAddresses"] = serde_json::json!([{ "value": email }]);
    }

    if let Some(ref phone) = req.phone {
        body["phoneNumbers"] = serde_json::json!([{ "value": phone }]);
    }

    if req.organization.is_some() || req.title.is_some() {
        let mut org = serde_json::Map::new();
        if let Some(ref name) = req.organization {
            org.insert("name".into(), serde_json::Value::String(name.clone()));
        }
        if let Some(ref title) = req.title {
            org.insert("title".into(), serde_json::Value::String(title.clone()));
        }
        body["organizations"] = serde_json::Value::Array(vec![serde_json::Value::Object(org)]);
    }

    body
}

/// Minimal percent-encoding for query parameter values.
fn urlencod(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build the full URL for a given API path.
    fn build_url(base: &str, path: &str) -> String {
        format!("{base}{path}")
    }

    #[test]
    fn test_contact_deserialization() {
        let json = r#"{
            "resourceName": "people/c1234567890",
            "displayName": "Jane Doe",
            "emailAddresses": [
                { "value": "jane@example.com", "type": "work" }
            ],
            "phoneNumbers": [
                { "value": "+15551234567", "type": "mobile" }
            ],
            "organizations": [
                { "name": "Acme Corp", "title": "Engineer" }
            ]
        }"#;
        let contact: Contact = serde_json::from_str(json).unwrap();
        assert_eq!(contact.resource_name, "people/c1234567890");
        assert_eq!(contact.display_name.as_deref(), Some("Jane Doe"));
        assert_eq!(contact.email_addresses.len(), 1);
        assert_eq!(contact.email_addresses[0].value, "jane@example.com");
        assert_eq!(contact.email_addresses[0].r#type.as_deref(), Some("work"));
        assert_eq!(contact.phone_numbers[0].value, "+15551234567");
        assert_eq!(contact.organizations[0].name.as_deref(), Some("Acme Corp"));
        assert_eq!(contact.organizations[0].title.as_deref(), Some("Engineer"));
    }

    #[test]
    fn test_contact_deserialization_minimal() {
        let json = r#"{
            "resourceName": "people/c999"
        }"#;
        let contact: Contact = serde_json::from_str(json).unwrap();
        assert_eq!(contact.resource_name, "people/c999");
        assert!(contact.display_name.is_none());
        assert!(contact.email_addresses.is_empty());
        assert!(contact.phone_numbers.is_empty());
        assert!(contact.organizations.is_empty());
    }

    #[test]
    fn test_contact_list_deserialization() {
        let json = r#"{
            "connections": [
                {
                    "resourceName": "people/c1",
                    "displayName": "Alice",
                    "emailAddresses": [],
                    "phoneNumbers": [],
                    "organizations": []
                },
                {
                    "resourceName": "people/c2",
                    "displayName": "Bob",
                    "emailAddresses": [],
                    "phoneNumbers": [],
                    "organizations": []
                }
            ],
            "nextPageToken": "token_abc",
            "totalPeople": 42
        }"#;
        let list: ContactList = serde_json::from_str(json).unwrap();
        assert_eq!(list.connections.len(), 2);
        assert_eq!(list.next_page_token.as_deref(), Some("token_abc"));
        assert_eq!(list.total_people, Some(42));
    }

    #[test]
    fn test_contact_list_empty() {
        let json = r#"{ "connections": [] }"#;
        let list: ContactList = serde_json::from_str(json).unwrap();
        assert!(list.connections.is_empty());
        assert!(list.next_page_token.is_none());
        assert!(list.total_people.is_none());
    }

    #[test]
    fn test_contact_list_missing_connections() {
        let json = r#"{}"#;
        let list: ContactList = serde_json::from_str(json).unwrap();
        assert!(list.connections.is_empty());
    }

    #[test]
    fn test_contact_serialization_roundtrip() {
        let contact = Contact {
            resource_name: "people/c42".into(),
            display_name: Some("Test User".into()),
            email_addresses: vec![EmailAddress {
                value: "test@example.com".into(),
                r#type: Some("home".into()),
            }],
            phone_numbers: vec![PhoneNumber {
                value: "+44123456".into(),
                r#type: Some("work".into()),
            }],
            organizations: vec![Organization {
                name: Some("TestCo".into()),
                title: Some("CEO".into()),
            }],
        };
        let json = serde_json::to_string(&contact).unwrap();
        let back: Contact = serde_json::from_str(&json).unwrap();
        assert_eq!(back.resource_name, "people/c42");
        assert_eq!(back.display_name.as_deref(), Some("Test User"));
        assert_eq!(back.email_addresses[0].value, "test@example.com");
        assert_eq!(back.phone_numbers[0].value, "+44123456");
        assert_eq!(back.organizations[0].name.as_deref(), Some("TestCo"));
    }

    #[test]
    fn test_create_contact_request_serialization() {
        let req = CreateContactRequest {
            given_name: "Jane".into(),
            family_name: Some("Smith".into()),
            email: Some("jane@test.com".into()),
            phone: Some("+1555000".into()),
            organization: Some("Widgets Inc".into()),
            title: Some("CTO".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateContactRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.given_name, "Jane");
        assert_eq!(back.family_name.as_deref(), Some("Smith"));
        assert_eq!(back.email.as_deref(), Some("jane@test.com"));
        assert_eq!(back.phone.as_deref(), Some("+1555000"));
        assert_eq!(back.organization.as_deref(), Some("Widgets Inc"));
        assert_eq!(back.title.as_deref(), Some("CTO"));
    }

    #[test]
    fn test_client_default_base_url() {
        let client = GoogleContactsClient::new("tok");
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn test_client_custom_base_url_strips_slash() {
        let client =
            GoogleContactsClient::with_base_url("tok", "https://contacts.test/v1/");
        assert_eq!(client.base_url(), "https://contacts.test/v1");
    }

    #[test]
    fn test_list_contacts_url_construction() {
        let client = GoogleContactsClient::new("tok");
        let url = build_url(
            client.base_url(),
            "/people/me/connections?pageSize=25&personFields=names,emailAddresses,phoneNumbers,organizations",
        );
        assert!(url.starts_with(DEFAULT_BASE_URL));
        assert!(url.contains("pageSize=25"));
        assert!(url.contains("personFields="));
    }

    #[test]
    fn test_get_contact_url_construction() {
        let client = GoogleContactsClient::new("tok");
        let url = build_url(
            client.base_url(),
            "/people/c1234567890?personFields=names,emailAddresses,phoneNumbers,organizations",
        );
        assert!(url.contains("/people/c1234567890"));
        assert!(url.contains("personFields="));
    }

    #[test]
    fn test_search_contacts_url_construction() {
        let client = GoogleContactsClient::new("tok");
        let query = urlencod("Jane Doe");
        let url = build_url(
            client.base_url(),
            &format!(
                "/people:searchContacts?query={}&pageSize=10&readMask=names,emailAddresses,phoneNumbers,organizations",
                query
            ),
        );
        assert!(url.contains("query=Jane%20Doe"));
        assert!(url.contains("pageSize=10"));
    }

    #[test]
    fn test_delete_contact_url_construction() {
        let client = GoogleContactsClient::new("tok");
        let url = build_url(client.base_url(), "/people/c42:deleteContact");
        assert!(url.contains("/people/c42:deleteContact"));
    }

    #[test]
    fn test_build_contact_body_full() {
        let req = CreateContactRequest {
            given_name: "Alice".into(),
            family_name: Some("Wonder".into()),
            email: Some("alice@example.com".into()),
            phone: Some("+1234".into()),
            organization: Some("WonderCo".into()),
            title: Some("VP".into()),
        };
        let body = build_contact_body(&req);
        assert_eq!(body["names"][0]["givenName"], "Alice");
        assert_eq!(body["names"][0]["familyName"], "Wonder");
        assert_eq!(body["emailAddresses"][0]["value"], "alice@example.com");
        assert_eq!(body["phoneNumbers"][0]["value"], "+1234");
        assert_eq!(body["organizations"][0]["name"], "WonderCo");
        assert_eq!(body["organizations"][0]["title"], "VP");
    }

    #[test]
    fn test_build_contact_body_minimal() {
        let req = CreateContactRequest {
            given_name: "Bob".into(),
            family_name: None,
            email: None,
            phone: None,
            organization: None,
            title: None,
        };
        let body = build_contact_body(&req);
        assert_eq!(body["names"][0]["givenName"], "Bob");
        assert!(body.get("emailAddresses").is_none());
        assert!(body.get("phoneNumbers").is_none());
        assert!(body.get("organizations").is_none());
    }

    #[test]
    fn test_urlencod_special_characters() {
        assert_eq!(urlencod("hello world"), "hello%20world");
        assert_eq!(urlencod("a+b=c"), "a%2Bb%3Dc");
        assert_eq!(urlencod("safe-_.~"), "safe-_.~");
    }

    #[test]
    fn test_search_result_deserialization() {
        let json = r#"{
            "results": [
                {
                    "person": {
                        "resourceName": "people/c100",
                        "displayName": "Found Person",
                        "emailAddresses": [{ "value": "found@test.com" }],
                        "phoneNumbers": [],
                        "organizations": []
                    }
                }
            ]
        }"#;
        let result: SearchResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].person.resource_name, "people/c100");
        assert_eq!(
            result.results[0].person.display_name.as_deref(),
            Some("Found Person")
        );
    }

    #[test]
    fn test_search_result_empty() {
        let json = r#"{ "results": [] }"#;
        let result: SearchResult = serde_json::from_str(json).unwrap();
        assert!(result.results.is_empty());
    }
}
