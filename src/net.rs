//! One shared HTTP agent (small memory footprint, keeps connections alive).
use std::sync::OnceLock;
use std::time::Duration;

static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

pub fn agent() -> &'static ureq::Agent {
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0")
            .build()
    })
}
