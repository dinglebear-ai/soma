#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayCodeModeHost {
    ui_links: Vec<String>,
}

impl GatewayCodeModeHost {
    pub fn capture_ui_link(&mut self, href: impl Into<String>) {
        self.ui_links.push(href.into());
    }

    #[must_use]
    pub fn ui_links(&self) -> &[String] {
        &self.ui_links
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
