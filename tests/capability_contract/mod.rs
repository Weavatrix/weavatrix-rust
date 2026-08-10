use crate::support::GitFixture;
use blazingly_json::{Value, json};
use weavatrix_rust::{Weavatrix, tools};

/// Two components, three served routes. `api` serves the users surface and
/// `admin` serves an audit route a contract may or may not claim.
pub(crate) fn repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write(
        "api/users.rs",
        "#[get(\"/users/{id}\", id = \"users.read\")]\n\
         async fn read_user() {}\n\
         #[post(\"/users\", id = \"users.create\")]\n\
         async fn create_user() {}\n",
    );
    fixture.write(
        "admin/audit.rs",
        "#[get(\"/audit\", id = \"audit.read\")]\nasync fn read_audit() {}\n",
    );
    fixture
}

pub(crate) fn contract(capabilities: &str) -> String {
    format!(
        r#"{{
          "architectureContractV": 1,
          "name": "Test",
          "components": [
            {{"id": "api", "name": "API", "paths": ["api"]}},
            {{"id": "admin", "name": "Admin", "paths": ["admin"]}}
          ],
          "capabilities": {capabilities},
          "dependencyRules": [],
          "exceptions": []
        }}"#
    )
}

pub(crate) fn verify(fixture: &GitFixture, args: Value) -> Value {
    let mut engine = Weavatrix::open(&fixture.root).unwrap();
    tools::call(&mut engine, "verify_capabilities", args).unwrap()
}

pub(crate) fn call(fixture: &GitFixture) -> Value {
    verify(fixture, json!({}))
}

/// The capability ids reported in one finding section, sorted.
pub(crate) fn codes(report: &Value, section: &str) -> Vec<String> {
    sorted(report, section, "capability")
}

/// The endpoints reported as claimed by nothing, sorted.
pub(crate) fn unmapped(report: &Value) -> Vec<String> {
    sorted(report, "unmapped", "endpoint")
}

fn sorted(report: &Value, section: &str, field: &str) -> Vec<String> {
    let mut values = report[section]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|item| item[field].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    values.sort();
    values
}
