mod calls;
mod integrations;

pub(crate) use calls::{graph_calls, health_source_calls};
pub(crate) use integrations::{coverage_formats, git_calls, memory_call, semantic_calls};

use crate::support::GitFixture;

pub(crate) fn repository() -> GitFixture {
    let fixture = GitFixture::new();
    fixture.write("lib/util.js", "export function helper(){ return 1; }\n");
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\nimport * as natsLib from 'nats';\nconst nc = natsLib.connect();\nexport function list(){ return helper(); }\nrouter.get('/api/items', list);\nnc.publish('jobs', new Uint8Array());\n",
    );
    let clone = "export function duplicate(value){ const a=value+1; const b=a*2; const c=b-3; return c+a+b+value; }\n";
    fixture.write("app/clone-a.js", clone);
    fixture.write("app/clone-b.js", clone);
    fixture.write(
        "package.json",
        r#"{"dependencies":{"unused":"1.0.0","express":"1.0.0"}}"#,
    );
    fixture.write(
        ".weavatrix/architecture.json",
        r#"{"components":[{"id":"app","paths":["app"]},{"id":"lib","paths":["lib"]}],"dependencyRules":[{"id":"no-app-lib","action":"forbid","from":["app"],"to":["lib"],"kinds":["imports"]}],"ratchet":{"baseline":{"fingerprints":[]}}}"#,
    );
    fixture.commit("baseline");
    fixture.write(
        "app/main.js",
        "import { helper } from '../lib/util.js';\nimport * as natsLib from 'nats';\nconst nc = natsLib.connect();\nexport function list(){ return helper()+1; }\nrouter.get('/api/items', list);\nnc.publish('jobs', new Uint8Array());\n",
    );
    fixture.commit("change");
    fixture
}
