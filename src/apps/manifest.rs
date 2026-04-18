//! Command manifest: transforms MCP discovery cache into a CLI command tree.
//!
//! The manifest is the single source of truth for the dynamic CLI surface.
//! It is built from cached `DiscoveryInventoryView` items and an optional
//! profile overlay, producing a flat/grouped command set with typed flags.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::DiscoveryInventoryView;

// ---------------------------------------------------------------------------
// Fuzzy command matching
// ---------------------------------------------------------------------------

/// Compute Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let b_len = b.len();
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

/// Find the closest command names in the manifest for a given query.
/// Returns suggestions within a reasonable edit distance (≤ 3, and < half the query length).
pub fn fuzzy_suggest(query: &str, manifest: &CommandManifest) -> Vec<String> {
    let max_distance = 3.min(query.len() / 2 + 1);
    let mut candidates: Vec<(usize, String)> = manifest
        .commands
        .keys()
        .filter_map(|name| {
            let dist = levenshtein(query, name);
            if dist > 0 && dist <= max_distance {
                Some((dist, name.clone()))
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by_key(|(dist, _)| *dist);
    candidates
        .into_iter()
        .map(|(_, name)| name)
        .take(3)
        .collect()
}

// ---------------------------------------------------------------------------
// Core manifest types
// ---------------------------------------------------------------------------

/// A complete command manifest for one config/alias.
#[derive(Debug, Clone, Default)]
pub struct CommandManifest {
    /// Top-level commands, keyed by CLI name.
    pub commands: IndexMap<String, ManifestEntry>,
    /// Server display name (from config or negotiation).
    pub server_name: Option<String>,
}

/// A single entry in the manifest — either a leaf command or a group.
#[derive(Debug, Clone)]
pub enum ManifestEntry {
    Command(ManifestCommand),
    Group {
        summary: String,
        children: IndexMap<String, ManifestCommand>,
    },
}

/// A leaf command that maps to exactly one MCP operation.
#[derive(Debug, Clone)]
pub struct ManifestCommand {
    /// The kind of backing MCP primitive.
    pub kind: CommandKind,
    /// The original MCP name (tool name, prompt name, or resource URI).
    pub origin_name: String,
    /// One-line description from the server.
    pub summary: String,
    /// Typed flags derived from inputSchema or prompt arguments.
    pub flags: IndexMap<String, FlagSpec>,
    /// Optional positional argument (e.g. URI for resource templates).
    pub positional: Option<PositionalSpec>,
    /// Whether this command supports `--background`.
    pub supports_background: bool,
}

/// What kind of MCP primitive backs this command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    Tool,
    Resource,
    ResourceTemplate,
    Prompt,
}

impl CommandKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Resource => "resource",
            Self::ResourceTemplate => "resource_template",
            Self::Prompt => "prompt",
        }
    }
}

/// A typed CLI flag derived from JSON Schema or prompt argument metadata.
#[derive(Debug, Clone)]
pub struct FlagSpec {
    pub flag_type: FlagType,
    pub required: bool,
    pub default: Option<Value>,
    pub help: Option<String>,
    pub enum_values: Option<Vec<String>>,
}

/// Supported flag types (mapped from JSON Schema `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    /// Catch-all for complex/nested JSON — user passes raw JSON string.
    Json,
}

impl FlagType {
    pub fn value_name(&self) -> &'static str {
        match self {
            Self::String => "TEXT",
            Self::Integer => "INT",
            Self::Number => "NUM",
            Self::Boolean => "",
            Self::Array => "VAL,...",
            Self::Json => "JSON",
        }
    }
}

/// A positional argument (e.g. the URI for resource read or template param).
#[derive(Debug, Clone)]
pub struct PositionalSpec {
    pub name: String,
    pub help: Option<String>,
    pub required: bool,
}

// ---------------------------------------------------------------------------
// Profile overlay (optional per-server customization)
// ---------------------------------------------------------------------------

/// Optional per-server profile that can rename, hide, group, or alias commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileOverlay {
    /// Display name override for the help banner.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Rename commands: original_name → new_name.
    #[serde(default)]
    pub aliases: IndexMap<String, String>,
    /// Hide these commands from help and ls.
    #[serde(default)]
    pub hide: Vec<String>,
    /// Custom grouping: group_name → list of member commands.
    #[serde(default)]
    pub groups: IndexMap<String, Vec<String>>,
    /// Rename flags: command_name → { old_flag → new_flag }.
    #[serde(default)]
    pub flags: IndexMap<String, IndexMap<String, String>>,
    /// Override the resource read verb (default: "get").
    #[serde(default)]
    pub resource_verb: Option<String>,
}

// ---------------------------------------------------------------------------
// Builder — discovery inventory → manifest
// ---------------------------------------------------------------------------

impl CommandManifest {
    /// Build a manifest from cached discovery inventory.
    pub fn from_inventory(inventory: &DiscoveryInventoryView) -> Self {
        let mut flat_commands: IndexMap<String, ManifestCommand> = IndexMap::new();

        // Tools → commands
        if let Some(tools) = &inventory.tools {
            for tool in tools {
                if let Some(cmd) = tool_to_command(tool) {
                    flat_commands.insert(cmd.origin_name.clone(), cmd);
                }
            }
        }

        // Resources → unified "get" command
        // We don't create a command per resource — they're all accessed via "get <URI>"
        // But we record the resource list for completion
        let has_resources = inventory
            .resources
            .as_ref()
            .map(|r| !r.is_empty())
            .unwrap_or(false);
        if has_resources {
            flat_commands.insert(
                "get".to_owned(),
                ManifestCommand {
                    kind: CommandKind::Resource,
                    origin_name: "get".to_owned(),
                    summary: "Fetch a resource by URI".to_owned(),
                    flags: IndexMap::new(),
                    positional: Some(PositionalSpec {
                        name: "uri".to_owned(),
                        help: Some("Resource URI to fetch".to_owned()),
                        required: true,
                    }),
                    supports_background: false,
                },
            );
        }

        // Resource templates → individual commands
        if let Some(templates) = &inventory.resource_templates {
            for template in templates {
                if let Some(cmd) = resource_template_to_command(template) {
                    // Use the template's name (if provided) or sanitized URI as the command key
                    let key = template
                        .get("name")
                        .and_then(Value::as_str)
                        .map(sanitize_command_name)
                        .unwrap_or_else(|| sanitize_command_name(&cmd.origin_name));
                    flat_commands.insert(key, cmd);
                }
            }
        }

        // Prompts → commands
        if let Some(prompts) = &inventory.prompts {
            for prompt in prompts {
                if let Some(cmd) = prompt_to_command(prompt) {
                    flat_commands.insert(cmd.origin_name.clone(), cmd);
                }
            }
        }

        // Group by shared prefix (dotted names → subcommands)
        let commands = group_by_prefix(flat_commands);

        CommandManifest {
            commands,
            server_name: None,
        }
    }

    /// Apply a profile overlay to refine the manifest.
    pub fn apply_profile(&mut self, profile: &ProfileOverlay) {
        if let Some(name) = &profile.display_name {
            self.server_name = Some(name.clone());
        }

        // Rename resource verb if specified
        if let Some(verb) = &profile.resource_verb
            && let Some(entry) = self.commands.swap_remove("get")
        {
            self.commands.insert(verb.clone(), entry);
        }

        // Apply renames
        for (old_name, new_name) in &profile.aliases {
            // Try top-level rename first
            if let Some(entry) = self.commands.swap_remove(old_name) {
                self.commands.insert(new_name.clone(), entry);
                continue;
            }
            // Dotted alias: rename a child within a group (e.g. "create.payload" → "object")
            if let Some(dot_pos) = old_name.find('.') {
                let group = &old_name[..dot_pos];
                let child = &old_name[dot_pos + 1..];
                // Determine the new child name: if new_name contains a dot with the
                // same group prefix, strip it; otherwise use new_name as-is.
                let new_child = new_name
                    .strip_prefix(&format!("{}.", group))
                    .unwrap_or(new_name);
                if let Some(ManifestEntry::Group { children, .. }) = self.commands.get_mut(group)
                    && let Some(cmd) = children.swap_remove(child)
                {
                    children.insert(new_child.to_owned(), cmd);
                }
            }
        }

        // Hide commands
        for name in &profile.hide {
            self.commands.swap_remove(name);
        }

        // Apply custom groups
        for (group_name, members) in &profile.groups {
            let mut children = IndexMap::new();
            for member in members {
                // Look for member in top-level commands
                if let Some(ManifestEntry::Command(cmd)) = self.commands.swap_remove(member) {
                    let child_name = member
                        .strip_prefix(&format!("{}.", group_name))
                        .unwrap_or(member)
                        .to_owned();
                    children.insert(child_name, cmd);
                }
            }
            if !children.is_empty() {
                self.commands.insert(
                    group_name.clone(),
                    ManifestEntry::Group {
                        summary: format!("{} commands", group_name),
                        children,
                    },
                );
            }
        }

        // Apply flag renames
        for (cmd_name, flag_renames) in &profile.flags {
            if let Some(ManifestEntry::Command(cmd)) = self.commands.get_mut(cmd_name) {
                for (old_flag, new_flag) in flag_renames {
                    if let Some(spec) = cmd.flags.swap_remove(old_flag) {
                        cmd.flags.insert(new_flag.clone(), spec);
                    }
                }
            }
        }
    }

    /// Get all cached resource URIs for shell completion.
    pub fn resource_uris(inventory: &DiscoveryInventoryView) -> Vec<String> {
        inventory
            .resources
            .as_ref()
            .map(|resources| {
                resources
                    .iter()
                    .filter_map(|r| r.get("uri").and_then(Value::as_str).map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn tool_to_command(tool: &Value) -> Option<ManifestCommand> {
    let name = tool
        .get("id")
        .or_else(|| tool.get("name"))
        .and_then(Value::as_str)?
        .to_owned();
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let flags = extract_flags_from_schema(tool.get("inputSchema"));

    Some(ManifestCommand {
        kind: CommandKind::Tool,
        origin_name: name,
        summary: description,
        flags,
        positional: None,
        supports_background: true,
    })
}

fn resource_template_to_command(template: &Value) -> Option<ManifestCommand> {
    let uri_template = template.get("uriTemplate").and_then(Value::as_str)?;
    let name = template
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(uri_template);
    let description = template
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    // Extract template parameters from URI template (RFC 6570 basic)
    let params = extract_uri_template_params(uri_template);

    let mut flags = IndexMap::new();
    for param in &params {
        flags.insert(
            param.clone(),
            FlagSpec {
                flag_type: FlagType::String,
                required: true,
                default: None,
                help: Some(format!("Template parameter '{}'", param)),
                enum_values: None,
            },
        );
    }

    // Use the template name (cleaned) as the command name
    let _cmd_name = sanitize_command_name(name);

    Some(ManifestCommand {
        kind: CommandKind::ResourceTemplate,
        origin_name: uri_template.to_owned(),
        summary: description,
        flags,
        positional: if params.len() == 1 {
            Some(PositionalSpec {
                name: params[0].clone(),
                help: Some(format!("Value for '{}'", params[0])),
                required: true,
            })
        } else {
            None
        },
        supports_background: false,
    })
}

fn prompt_to_command(prompt: &Value) -> Option<ManifestCommand> {
    let name = prompt.get("name").and_then(Value::as_str)?.to_owned();
    let description = prompt
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let mut flags = IndexMap::new();
    if let Some(arguments) = prompt.get("arguments").and_then(Value::as_array) {
        for arg in arguments {
            // Arguments can be either strings or objects with name/required/description
            if let Some(arg_name) = arg.as_str() {
                flags.insert(
                    to_flag_name(arg_name),
                    FlagSpec {
                        flag_type: FlagType::String,
                        required: false,
                        default: None,
                        help: None,
                        enum_values: None,
                    },
                );
            } else if let Some(arg_name) = arg.get("name").and_then(Value::as_str) {
                let required = arg
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let help = arg
                    .get("description")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                flags.insert(
                    to_flag_name(arg_name),
                    FlagSpec {
                        flag_type: FlagType::String,
                        required,
                        default: None,
                        help,
                        enum_values: None,
                    },
                );
            }
        }
    }

    Some(ManifestCommand {
        kind: CommandKind::Prompt,
        origin_name: name,
        summary: description,
        flags,
        positional: None,
        supports_background: false,
    })
}

/// Extract typed flags from a JSON Schema `inputSchema`.
fn extract_flags_from_schema(schema: Option<&Value>) -> IndexMap<String, FlagSpec> {
    let mut flags = IndexMap::new();
    let Some(schema) = schema else {
        return flags;
    };
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return flags;
    };
    let required_set: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    for (prop_name, prop_schema) in properties {
        let flag_name = to_flag_name(prop_name);
        let flag_type = json_schema_type_to_flag_type(prop_schema);
        let required = required_set.contains(&prop_name.as_str());
        let default = prop_schema.get("default").cloned();
        let help = prop_schema
            .get("description")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let enum_values = prop_schema
            .get("enum")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            });

        flags.insert(
            flag_name,
            FlagSpec {
                flag_type,
                required,
                default,
                help,
                enum_values,
            },
        );
    }

    flags
}

fn json_schema_type_to_flag_type(schema: &Value) -> FlagType {
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => FlagType::String,
        Some("integer") => FlagType::Integer,
        Some("number") => FlagType::Number,
        Some("boolean") => FlagType::Boolean,
        Some("array") => FlagType::Array,
        Some("object") => FlagType::Json,
        _ => FlagType::String,
    }
}

/// Extract parameter names from a URI template (RFC 6570 basic).
/// e.g. "mail://search?q={query}&folder={folder}" → ["query", "folder"]
fn extract_uri_template_params(template: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        if let Some(end) = rest[start..].find('}') {
            let param = &rest[start + 1..start + end];
            // Skip URI template operators (+, #, etc.)
            let param = param.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !param.is_empty() {
                params.push(param.to_owned());
            }
            rest = &rest[start + end + 1..];
        } else {
            break;
        }
    }
    params
}

/// Convert a property name to a CLI flag name (kebab-case).
/// "maxTokens" → "max-tokens", "message_type" → "message-type"
fn to_flag_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' || ch == '.' {
            result.push('-');
        } else if ch.is_uppercase() && i > 0 {
            result.push('-');
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Sanitize a name for use as a CLI command name.
fn sanitize_command_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

/// Group flat commands by shared dotted prefix.
/// e.g. "email.send", "email.reply" → group "email" { "send", "reply" }
fn group_by_prefix(flat: IndexMap<String, ManifestCommand>) -> IndexMap<String, ManifestEntry> {
    // Count prefix occurrences
    let mut prefix_counts: IndexMap<String, Vec<String>> = IndexMap::new();
    for name in flat.keys() {
        if let Some(dot_pos) = name.find('.') {
            let prefix: &str = &name[..dot_pos];
            prefix_counts
                .entry(prefix.to_owned())
                .or_default()
                .push(name.clone());
        }
    }

    let mut result: IndexMap<String, ManifestEntry> = IndexMap::new();
    let mut grouped_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Create groups for prefixes with ≥2 members
    for (prefix, members) in &prefix_counts {
        if members.len() >= 2 {
            let mut children: IndexMap<String, ManifestCommand> = IndexMap::new();
            for member in members {
                if let Some(cmd) = flat.get(member) {
                    let pfx_dot = format!("{}.", prefix);
                    let suffix = member.strip_prefix(&pfx_dot).unwrap_or(member);
                    children.insert(suffix.to_owned(), cmd.clone());
                    grouped_names.insert(member.clone());
                }
            }
            if !children.is_empty() {
                result.insert(
                    prefix.clone(),
                    ManifestEntry::Group {
                        summary: format!("{} commands", prefix),
                        children,
                    },
                );
            }
        }
    }

    // Add ungrouped commands directly
    for (name, cmd) in &flat {
        if !grouped_names.contains(name) {
            result.insert(name.clone(), ManifestEntry::Command(cmd.clone()));
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_inventory() -> DiscoveryInventoryView {
        DiscoveryInventoryView {
            config_name: "test".to_owned(),
            app_id: "test".to_owned(),
            tools: Some(vec![
                json!({
                    "id": "echo",
                    "kind": "tool",
                    "description": "Echoes back the input",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "message": { "type": "string", "description": "Message to echo" }
                        },
                        "required": ["message"]
                    }
                }),
                json!({
                    "id": "add",
                    "kind": "tool",
                    "description": "Adds two numbers",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "a": { "type": "number" },
                            "b": { "type": "number" }
                        },
                        "required": ["a", "b"]
                    }
                }),
                json!({
                    "id": "email.send",
                    "kind": "tool",
                    "description": "Send an email",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "to": { "type": "string", "description": "Recipient" },
                            "subject": { "type": "string", "description": "Subject line" },
                            "body": { "type": "string" }
                        },
                        "required": ["to", "subject"]
                    }
                }),
                json!({
                    "id": "email.reply",
                    "kind": "tool",
                    "description": "Reply to an email"
                }),
            ]),
            resources: Some(vec![
                json!({ "uri": "mail://inbox", "kind": "resource", "description": "Inbox" }),
            ]),
            resource_templates: Some(vec![json!({
                "uriTemplate": "mail://search?q={query}",
                "name": "mail-search",
                "description": "Search messages",
                "kind": "resource_template"
            })]),
            prompts: Some(vec![json!({
                "name": "summarize",
                "description": "Summarize a thread",
                "arguments": [
                    { "name": "thread_id", "required": true, "description": "Thread to summarize" },
                    { "name": "style", "required": false, "description": "Summary style" }
                ]
            })]),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn builds_manifest_from_inventory() {
        let inventory = test_inventory();
        let manifest = CommandManifest::from_inventory(&inventory);

        // Should have: echo, add, email (group), get, mail-search, summarize
        assert!(manifest.commands.contains_key("echo"));
        assert!(manifest.commands.contains_key("add"));
        assert!(manifest.commands.contains_key("email"));
        assert!(manifest.commands.contains_key("get"));
        assert!(manifest.commands.contains_key("mail-search"));
        assert!(manifest.commands.contains_key("summarize"));
    }

    #[test]
    fn tool_flags_from_schema() {
        let inventory = test_inventory();
        let manifest = CommandManifest::from_inventory(&inventory);

        if let Some(ManifestEntry::Command(cmd)) = manifest.commands.get("echo") {
            assert_eq!(cmd.flags.len(), 1);
            assert!(cmd.flags.contains_key("message"));
            assert!(cmd.flags["message"].required);
            assert_eq!(cmd.flags["message"].flag_type, FlagType::String);
        } else {
            panic!("echo should be a Command");
        }
    }

    #[test]
    fn dotted_names_create_groups() {
        let inventory = test_inventory();
        let manifest = CommandManifest::from_inventory(&inventory);

        match manifest.commands.get("email") {
            Some(ManifestEntry::Group { children, .. }) => {
                assert!(children.contains_key("send"));
                assert!(children.contains_key("reply"));
            }
            other => panic!("email should be a Group, got {:?}", other),
        }
    }

    #[test]
    fn prompt_flags_from_arguments() {
        let inventory = test_inventory();
        let manifest = CommandManifest::from_inventory(&inventory);

        if let Some(ManifestEntry::Command(cmd)) = manifest.commands.get("summarize") {
            assert_eq!(cmd.kind, CommandKind::Prompt);
            assert!(cmd.flags.contains_key("thread-id"));
            assert!(cmd.flags["thread-id"].required);
            assert!(cmd.flags.contains_key("style"));
            assert!(!cmd.flags["style"].required);
        } else {
            panic!("summarize should be a Command");
        }
    }

    #[test]
    fn resource_template_extracts_params() {
        let params = extract_uri_template_params("mail://search?q={query}&folder={folder}");
        assert_eq!(params, vec!["query", "folder"]);
    }

    #[test]
    fn to_flag_name_converts_cases() {
        assert_eq!(to_flag_name("maxTokens"), "max-tokens");
        assert_eq!(to_flag_name("message_type"), "message-type");
        assert_eq!(to_flag_name("context.thread_id"), "context-thread-id");
        assert_eq!(to_flag_name("simple"), "simple");
    }

    #[test]
    fn profile_overlay_renames() {
        let inventory = test_inventory();
        let mut manifest = CommandManifest::from_inventory(&inventory);
        let profile = ProfileOverlay {
            aliases: IndexMap::from([("echo".to_owned(), "ping".to_owned())]),
            hide: vec!["add".to_owned()],
            resource_verb: Some("fetch".to_owned()),
            ..Default::default()
        };
        manifest.apply_profile(&profile);

        assert!(manifest.commands.contains_key("ping"));
        assert!(!manifest.commands.contains_key("echo"));
        assert!(!manifest.commands.contains_key("add"));
        assert!(manifest.commands.contains_key("fetch"));
        assert!(!manifest.commands.contains_key("get"));
    }

    #[test]
    fn profile_overlay_renames_grouped_subcommand() {
        let inventory = test_inventory();
        let mut manifest = CommandManifest::from_inventory(&inventory);

        // email.send and email.reply should be grouped under "email"
        assert!(manifest.commands.contains_key("email"));
        if let Some(ManifestEntry::Group { children, .. }) = manifest.commands.get("email") {
            assert!(children.contains_key("send"), "should have child 'send'");
            assert!(children.contains_key("reply"), "should have child 'reply'");
        } else {
            panic!("email should be a Group");
        }

        // Rename email.send → email.compose via dotted alias
        let profile = ProfileOverlay {
            aliases: IndexMap::from([("email.send".to_owned(), "compose".to_owned())]),
            ..Default::default()
        };
        manifest.apply_profile(&profile);

        if let Some(ManifestEntry::Group { children, .. }) = manifest.commands.get("email") {
            assert!(!children.contains_key("send"), "'send' should be renamed");
            assert!(
                children.contains_key("compose"),
                "should have child 'compose'"
            );
            assert!(
                children.contains_key("reply"),
                "'reply' should be untouched"
            );
        } else {
            panic!("email should still be a Group");
        }
    }

    #[test]
    fn profile_overlay_renames_grouped_subcommand_with_dotted_new_name() {
        let inventory = test_inventory();
        let mut manifest = CommandManifest::from_inventory(&inventory);

        // Rename email.reply → email.respond (dotted new name with same group prefix)
        let profile = ProfileOverlay {
            aliases: IndexMap::from([("email.reply".to_owned(), "email.respond".to_owned())]),
            ..Default::default()
        };
        manifest.apply_profile(&profile);

        if let Some(ManifestEntry::Group { children, .. }) = manifest.commands.get("email") {
            assert!(!children.contains_key("reply"), "'reply' should be renamed");
            assert!(
                children.contains_key("respond"),
                "should have child 'respond'"
            );
        } else {
            panic!("email should still be a Group");
        }
    }

    #[test]
    fn fuzzy_suggest_returns_close_matches() {
        let mut manifest = CommandManifest::default();
        manifest.commands.insert(
            "echo".to_owned(),
            ManifestEntry::Command(ManifestCommand {
                kind: CommandKind::Tool,
                origin_name: "echo".to_owned(),
                summary: "".to_owned(),
                flags: IndexMap::new(),
                positional: None,
                supports_background: false,
            }),
        );
        manifest.commands.insert(
            "search".to_owned(),
            ManifestEntry::Command(ManifestCommand {
                kind: CommandKind::Tool,
                origin_name: "search".to_owned(),
                summary: "".to_owned(),
                flags: IndexMap::new(),
                positional: None,
                supports_background: false,
            }),
        );

        let suggestions = super::fuzzy_suggest("ecoh", &manifest);
        assert!(
            suggestions.contains(&"echo".to_owned()),
            "should suggest 'echo' for 'ecoh'"
        );

        let suggestions = super::fuzzy_suggest("serch", &manifest);
        assert!(
            suggestions.contains(&"search".to_owned()),
            "should suggest 'search' for 'serch'"
        );

        let suggestions = super::fuzzy_suggest("zzzzz", &manifest);
        assert!(
            suggestions.is_empty(),
            "should not suggest anything for 'zzzzz'"
        );
    }

    #[test]
    fn levenshtein_basic_cases() {
        assert_eq!(super::levenshtein("kitten", "sitting"), 3);
        assert_eq!(super::levenshtein("echo", "ecoh"), 2);
        assert_eq!(super::levenshtein("abc", "abc"), 0);
        assert_eq!(super::levenshtein("", "abc"), 3);
        assert_eq!(super::levenshtein("abc", ""), 3);
    }
}
