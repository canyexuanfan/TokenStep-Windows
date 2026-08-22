# -*- coding: utf-8 -*-
import io, re

# ── paths.rs: Cursor paths ──
t = io.open('paths.rs', encoding='utf-8').read()
add = '''
// ── Cursor (upstream v0.2.1/v0.2.2) ───────────────────────────────────────

/// Cursor's VS Code state DB (holds the auth token in ItemTable), used for
/// the official usage API. Windows: %APPDATA%/Cursor/User/globalStorage/state.vscdb
pub fn cursor_state_vscdb() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home_dir().join("AppData").join("Roaming"))
        .join("Cursor")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

/// Cursor's local code-signal DB: ~/.cursor/ai-tracking/ai-code-tracking.db
pub fn cursor_code_tracking_db() -> PathBuf {
    home_dir().join(".cursor").join("ai-tracking").join("ai-code-tracking.db")
}

/// cache/cursor-usage-cache.json — official usage events cache.
pub fn cursor_usage_cache_json() -> PathBuf {
    app_support_root().join("cache").join("cursor-usage-cache.json")
}
'''
if 'cursor_state_vscdb' not in t:
    t = t.rstrip() + '\n' + add
    io.open('paths.rs', 'w', encoding='utf-8', newline='\n').write(t)
    print('paths.rs extended')

# ── models.rs ──
m = io.open('models.rs', encoding='utf-8').read()

m = re.sub(r'/// A sanitized project aggregate.*?\n\}\n\n', '', m, flags=re.S)
m = m.replace('''    /// Per-project breakdown for this day (upstream B1-lite). Older
    /// snapshots decode as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projects: Option<Vec<ProjectUsage>>,
''', '')
m = m.replace('''    /// Info about the collection attempt that produced this snapshot
    /// (upstream G-V1 freshness). Older snapshots decode as `None`.
    #[serde(default, rename = "source_attempt", skip_serializing_if = "Option::is_none")]
    pub source_attempt: Option<crate::freshness::RefreshAttemptRecord>,
    /// Project-dimension aggregates (upstream B1-lite, sanitized last path
    /// segment only). Older snapshots decode as empty.
    #[serde(default)]
    pub projects: Vec<ProjectUsage>,
}''', '''}''')
m = m.replace('''            agent_work: vec![],
            tools: vec![],
            models: vec![],
            sources: std::collections::BTreeMap::new(),
            source_attempt: None,
            projects: vec![],
        }''', '''            agent_work: vec![],
            tools: vec![],
            models: vec![],
            sources: std::collections::BTreeMap::new(),
        }''')

m = m.replace('''    #[serde(rename = "show_experimental_agent_sources", default)]
    pub show_experimental_agent_sources: bool,
}''', '''    #[serde(rename = "show_experimental_agent_sources", default)]
    pub show_experimental_agent_sources: bool,
    /// Official Cursor usage events join the today ring (upstream v0.2.1).
    #[serde(rename = "cursor_quota_enabled", default)]
    pub cursor_quota_enabled: bool,
    /// Cursor code-signal card (L3; today's code-production counts).
    #[serde(rename = "cursor_code_signal_enabled", default)]
    pub cursor_code_signal_enabled: bool,
    /// Per-provider quota probes (glm / kimi / grok), default off.
    #[serde(rename = "glm_quota_enabled", default)]
    pub glm_quota_enabled: bool,
    #[serde(rename = "kimi_quota_enabled", default)]
    pub kimi_quota_enabled: bool,
    #[serde(rename = "grok_quota_enabled", default)]
    pub grok_quota_enabled: bool,
}''')
m = m.replace('''            hidden_device_ids: vec![],
            show_experimental_agent_sources: false,
        }''', '''            hidden_device_ids: vec![],
            show_experimental_agent_sources: false,
            cursor_quota_enabled: false,
            cursor_code_signal_enabled: false,
            glm_quota_enabled: false,
            kimi_quota_enabled: false,
            grok_quota_enabled: false,
        }''')

io.open('models.rs', 'w', encoding='utf-8', newline='\n').write(m)
print('models.rs done; ProjectUsage refs left:', m.count('ProjectUsage'))
