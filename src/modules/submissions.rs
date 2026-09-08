//! VM submissions: the user's submitted VMs, the global queue, the
//! submission rules, and the submit form. All endpoints require a
//! logged-in session (`/submissions/*` redirects to `/login/`
//! otherwise).
//!
//! Form fields are scraped dynamically from `vm.php` (`parse_form_fields`)
//! so the POST always matches the site's current field names; a static
//! fallback list is used when parsing fails.

use anyhow::Result;
use scraper::{Html, Selector};

use crate::modules::session::HmvSession;

const SUBMISSIONS_PATH: &str = "/submissions/vm.php";
const STATUS_PATH: &str = "/submissions/status.php";
const RULES_PATH: &str = "/submissions/rules.php";

/// One row of the submissions table (status.php). The site shows every
/// user's submissions here; "mine" = rows whose user matches the account.
#[derive(Debug, Clone)]
pub struct QueueEntry {
    /// The account that submitted the VM (table's first column).
    pub user: String,
    pub name: String,
    pub date: String,
    /// Raw status text from the site ("pending", "accepted", ...).
    pub status: String,
    /// easy / medium / hard.
    pub level: String,
}

/// One editable form field scraped from the submit form.
#[derive(Debug, Clone)]
pub struct FormField {
    /// `name=` attribute used for the POST.
    pub name: String,
    /// Human label shown in the popup.
    pub label: String,
    pub kind: FieldKind,
    pub required: bool,
    /// Pre-filled value (hidden inputs carry theirs).
    pub value: String,
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    Text,
    Select(Vec<String>),
    Hidden,
}

/// Verdict of a VM submission.
#[derive(Debug, Clone)]
pub enum SubmitVerdict {
    /// Accepted and waiting in the queue.
    Submitted,
    Rejected(String),
    Unknown(String),
}

/// Static fallback matching the actual vm.php form
/// (vmname/url/flaguser/flagroot/tags/level select/notes/writeup).
pub fn fallback_form_fields() -> Vec<FormField> {
    vec![
        FormField {
            name: "vmname".into(),
            label: "VM name:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
        FormField {
            name: "url".into(),
            label: "Download URL:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
        FormField {
            name: "flaguser".into(),
            label: "User flag:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
        FormField {
            name: "flagroot".into(),
            label: "Root flag:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
        FormField {
            name: "tags".into(),
            label: "Tags:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
        FormField {
            name: "level".into(),
            label: "Level:".into(),
            kind: FieldKind::Select(vec!["easy".into(), "medium".into(), "hard".into()]),
            required: true,
            value: "easy".into(),
        },
        FormField {
            name: "notes".into(),
            label: "Notes:".into(),
            kind: FieldKind::Text,
            required: false,
            value: String::new(),
        },
        FormField {
            name: "writeup".into(),
            label: "Writeup:".into(),
            kind: FieldKind::Text,
            required: true,
            value: String::new(),
        },
    ]
}

pub struct SubmissionManager {
    session: HmvSession,
}

impl SubmissionManager {
    pub fn new(session: HmvSession) -> Self {
        Self { session }
    }

    /// Fetches the submissions queue from status.php. When `user` is
    /// given, only that account's rows are returned ("my submissions");
    /// otherwise the whole queue.
    pub async fn fetch_queue(&self, user: Option<&str>) -> Result<Vec<QueueEntry>> {
        let html = self.session.get(STATUS_PATH).await?;
        let all = parse_queue(&html);
        Ok(match user {
            Some(u) => all
                .into_iter()
                .filter(|e| e.user.eq_ignore_ascii_case(u))
                .collect(),
            None => all,
        })
    }

    /// Fetches the submission rules as plain text. Network-only.
    pub async fn fetch_rules(&self) -> Result<String> {
        let html = self.session.get(RULES_PATH).await?;
        Ok(parse_rules(&html))
    }

    /// Scrapes the submit form fields from vm.php.
    pub async fn fetch_form(&self) -> Result<Vec<FormField>> {
        let html = self.session.get(SUBMISSIONS_PATH).await?;
        let fields = parse_form_fields(&html);
        if fields.is_empty() {
            Ok(fallback_form_fields())
        } else {
            Ok(fields)
        }
    }

    /// Submits the VM with the given (name, value) pairs — names must be
    /// the ones scraped from the form (or the fallback names).
    pub async fn submit(&self, fields: &[(String, String)]) -> Result<SubmitVerdict> {
        let form: Vec<(&str, &str)> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let body = self.session.post_form(SUBMISSIONS_PATH, &form).await?;
        let msg = body.to_lowercase();
        Ok(
            if msg.contains("success") || msg.contains("submitted") || msg.contains("correct") {
                SubmitVerdict::Submitted
            } else if msg.contains("error") || msg.contains("wrong") || msg.contains("not") {
                SubmitVerdict::Rejected(body.trim().chars().take(120).collect())
            } else {
                SubmitVerdict::Unknown(body.trim().chars().take(120).collect())
            },
        )
    }
}

/// Extracts the editable + hidden fields of the first `<form>` on the page.
pub fn parse_form_fields(html: &str) -> Vec<FormField> {
    let doc = Html::parse_document(html);
    let form_sel = match Selector::parse("form").ok() {
        Some(s) => s,
        None => return Vec::new(),
    };
    let Some(form) = doc.select(&form_sel).next() else {
        return Vec::new();
    };

    let input_sel = Selector::parse("input, select, textarea").unwrap();
    let option_sel = Selector::parse("option").unwrap();
    let label_sel = Selector::parse("label").unwrap();

    let mut fields = Vec::new();
    for el in form.select(&input_sel) {
        let name = match el.value().attr("name") {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => continue,
        };
        let tag = el.value().name();
        let input_type = el.value().attr("type").unwrap_or("text").to_lowercase();
        if tag == "input"
            && (input_type == "submit"
                || input_type == "button"
                || input_type == "reset"
                || input_type == "image")
        {
            continue;
        }

        let (kind, value) = if input_type == "hidden" {
            (
                FieldKind::Hidden,
                el.value().attr("value").unwrap_or("").to_string(),
            )
        } else if tag == "select" {
            let options: Vec<String> = el
                .select(&option_sel)
                .filter_map(|o| {
                    o.text()
                        .collect::<String>()
                        .trim()
                        .to_lowercase()
                        .into_option_string()
                })
                .collect();
            let selected = el
                .select(&option_sel)
                .find(|o| o.value().attr("selected").is_some())
                .map(|o| o.text().collect::<String>().trim().to_lowercase())
                .or_else(|| options.first().cloned())
                .unwrap_or_default();
            (FieldKind::Select(options), selected)
        } else {
            (
                FieldKind::Text,
                el.value().attr("value").unwrap_or("").to_string(),
            )
        };

        // Best-effort label: the <label for=name> preceding the field, else
        // prettified field name.
        let label = doc
            .select(&label_sel)
            .find(|l| l.value().attr("for").is_some_and(|f| f == name))
            .map(|l| l.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| prettify_name(&name));

        let required = el.value().attr("required").is_some() || label.ends_with('*');

        fields.push(FormField {
            name,
            label: format!("{}:", label.trim_end_matches(':').trim_end_matches('*')),
            kind,
            required,
            value,
        });
    }
    fields
}

/// Turns snake_case / camelCase field names into "Field name" labels.
fn prettify_name(name: &str) -> String {
    let spaced = name
        .replace('_', " ")
        .chars()
        .enumerate()
        .map(|(i, c)| if i == 0 { c.to_ascii_uppercase() } else { c })
        .collect::<String>();
    spaced
}

/// Helper: keeps empty option text out (trim + skip empties).
trait IntoOptionString {
    fn into_option_string(self) -> Option<String>;
}
impl IntoOptionString for String {
    fn into_option_string(self) -> Option<String> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

/// Parses the submissions queue from status.php. Actual site structure:
/// `th[scope=row]` holds the submitting user (profile link), then
/// `<td>` Vm name, Date, Status, Level, and a trailing rules link cell.
pub fn parse_queue(html: &str) -> Vec<QueueEntry> {
    let doc = Html::parse_document(html);
    let row_sel = match Selector::parse("table tbody tr").ok() {
        Some(s) => s,
        None => return Vec::new(),
    };
    let cell_sel = Selector::parse("td").unwrap();
    let user_sel = Selector::parse("th").unwrap();

    let mut out = Vec::new();
    for row in doc.select(&row_sel) {
        let user = row
            .select(&user_sel)
            .next()
            .map(|th| th.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();
        if cells.len() < 3 || user.is_empty() {
            continue;
        }
        out.push(QueueEntry {
            user,
            name: cells[0].clone(),
            date: cells.get(1).cloned().unwrap_or_default(),
            status: cells.get(2).cloned().unwrap_or_default(),
            level: cells.get(3).cloned().unwrap_or_default(),
        });
    }
    out
}

/// Parses the rules page. The rules render as <h1>VMs Rules</h1> followed
/// by <dl><dt>rule</dt><dd>explanation</dd>…</dl> — everything else
/// (nav, sidebar, greeting) is page chrome and is dropped.
pub fn parse_rules(html: &str) -> String {
    let doc = Html::parse_document(html);

    let dt_sel = Selector::parse("dt").unwrap();
    let dd_sel = Selector::parse("dd").unwrap();
    let dts: Vec<String> = doc
        .select(&dt_sel)
        .map(|el| {
            el.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let dds: Vec<String> = doc
        .select(&dd_sel)
        .map(|el| {
            el.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    if !dts.is_empty() {
        let mut lines = Vec::new();
        for (i, dt) in dts.iter().enumerate() {
            lines.push(dt.clone());
            if let Some(dd) = dds.get(i) {
                if !dd.is_empty() {
                    lines.push(format!("   {dd}"));
                }
            }
            lines.push(String::new());
        }
        return lines.join("\n");
    }

    // Fallback: whole body text (layout may change).
    doc.root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUEUE_FIXTURE: &str = r##"
    <html><body><table><tbody>
    <tr><th>User</th><th>Vm Name/Challenges</th><th>Date</th><th>Status</th><th>Level/Category</th></tr>
    <tr><th scope='row'><a href='/profile/?user=tester1'>tester1</a></th><td class='font-weight-bold'>Cacti</td><td>2026-09-06 17:40:42</td><td>pending</td><td>Hard</td><td><a href='rules.php'></a></td></tr>
    <tr><th scope='row'><a href='/profile/?user=demouser'>demouser</a></th><td>grenade</td><td>2026-09-03 09:34:59</td><td>pending</td><td>Easy</td><td><a href='rules.php'></a></td></tr>
    <tr><th scope='row'><a href='/profile/?user=demouser'>demouser</a></th><td>ledger</td><td>2026-09-02 03:04:17</td><td>pending</td><td>Easy</td><td><a href='rules.php'></a></td></tr>
    </tbody></table></body></html>"##;

    #[test]
    fn parses_queue_fixture() {
        let queue = parse_queue(QUEUE_FIXTURE);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue[0].user, "tester1");
        assert_eq!(queue[0].name, "Cacti");
        assert_eq!(queue[0].status, "pending");
        assert_eq!(queue[0].level, "Hard");
        // Filter by user returns only that account's rows.
        let mine: Vec<&QueueEntry> = queue
            .iter()
            .filter(|e| e.user.eq_ignore_ascii_case("demouser"))
            .collect();
        assert_eq!(mine.len(), 2);
        assert_eq!(mine[0].name, "grenade");
        assert_eq!(mine[1].name, "ledger");
    }

    const FORM_FIXTURE: &str = r##"
    <html><body>
    <form action="/submissions/vm.php" method="post">
      <input type="hidden" name="csrf" value="tok123">
      <label for="vmname">VM name*</label>
      <input type="text" name="vmname" id="vmname" required>
      <label for="level">Level*</label>
      <select name="level" id="level" required>
        <option>easy</option><option>medium</option><option>hard</option>
      </select>
      <input type="text" name="dlurl" id="dlurl" required>
      <label for="uflag">User flag*</label>
      <input type="text" name="uflag" id="uflag" required>
      <label for="rflag">Root flag*</label>
      <input type="text" name="rflag" id="rflag" required>
      <label for="wurl">Writeup URL*</label>
      <input type="text" name="wurl" id="wurl" required>
      <label for="tags">Tags*</label>
      <input type="text" name="tags" id="tags" required>
      <label for="notes">Notes</label>
      <textarea name="notes" id="notes"></textarea>
      <input type="submit" name="go" value="Send">
    </form>
    </body></html>"##;

    #[test]
    fn parses_form_fields_fixture() {
        let fields = parse_form_fields(FORM_FIXTURE);
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"csrf"));
        assert!(names.contains(&"vmname"));
        assert!(names.contains(&"level"));
        assert!(names.contains(&"notes"));
        assert!(!names.contains(&"go"), "submit button excluded");

        let level = fields.iter().find(|f| f.name == "level").unwrap();
        match &level.kind {
            FieldKind::Select(opts) => {
                assert_eq!(opts.len(), 3);
                assert!(opts.contains(&"medium".to_string()));
            }
            other => panic!("level should be a select, got {other:?}"),
        }
        let csrf = fields.iter().find(|f| f.name == "csrf").unwrap();
        assert!(matches!(csrf.kind, FieldKind::Hidden));
        assert_eq!(csrf.value, "tok123");
        let vmname = fields.iter().find(|f| f.name == "vmname").unwrap();
        assert!(vmname.required);
        assert_eq!(vmname.label, "VM name:");
        let notes = fields.iter().find(|f| f.name == "notes").unwrap();
        assert!(!notes.required);
    }

    const RULES_FIXTURE: &str = r#"
    <html><body><div class="container">
    <h1>VMs Rules</h1>
    <dl>
    <dt>1. Dont use external URLs as part of the challenge.</dt>
    <dd>If the external URL fails, VM cannot be completed.</dd>
    <dt>2. Avoid bruteforces that takes more than 10 mins.</dt>
    <dd>Avoid that bruteforcing SSH, FTP etc takes more than 10 minutes.</dd>
    </dl>
    </div></body></html>"#;

    #[test]
    fn parses_rules_fixture() {
        let rules = parse_rules(RULES_FIXTURE);
        assert!(rules.contains("1. Dont use external URLs"));
        assert!(rules.contains("If the external URL fails"));
        assert!(rules.contains("2. Avoid bruteforces"));
        assert!(!rules.contains("Dashboard"), "page chrome dropped");
    }

    #[test]
    fn fallback_fields_cover_owner_requirements() {
        let fields = fallback_form_fields();
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        for expected in [
            "vmname", "url", "flaguser", "flagroot", "tags", "level", "notes", "writeup",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        let notes = fields.iter().find(|f| f.name == "notes").unwrap();
        assert!(!notes.required, "notes optional");
        let others = fields.iter().filter(|f| f.name != "notes").count();
        assert_eq!(others, fields.len() - 1);
    }
}
