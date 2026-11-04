//! Shared site chrome used by layouts and loading boundaries.

use rusty::prelude::*;

/// Wraps page content in the site header and footer.
pub(crate) fn body(children: Element) -> Element {
    let home = crate::content::home_url();
    let docs = crate::content::url("docs");
    let about = crate::content::url("about");

    Element::fragment([
        view! { <a class="skip-link" href="#main">"Skip to content"</a> },
        view! {
            <header class="site-header">
                <a class="brand" href={home}>
                    <span class="brand-mark">"R"</span>
                    <span>"Rusty"</span>
                </a>
                <nav aria-label="Primary">
                    <a href={docs}>"Docs"</a>
                    <a href={about}>"About"</a>
                    <a href="https://github.com/hussain-ahmed2/rusty" target="_blank" rel="external">"GitHub"</a>
                </nav>
            </header>
        },
        view! { <div id="main" class="site-main">{children}</div> },
        view! {
            <footer class="site-footer">
                <p>"Built with Rusty. Static export deployed by GitHub Actions."</p>
                <p>"MIT OR Apache-2.0"</p>
            </footer>
        },
        Element::raw_html(SITE_SCRIPT),
    ])
}

/// Renders the documentation sidebar placeholder.
pub(crate) fn docs_sidebar_skeleton() -> Element {
    let items = Element::fragment(
        (0..=crate::content::DOCS.len())
            .map(|_| view! { <div class="skeleton skeleton-nav"></div> })
            .collect::<Vec<_>>(),
    );
    view! {
        <aside class="docs-sidebar" aria-hidden="true">
            <div class="skeleton skeleton-eyebrow"></div>
            <div class="skeleton-list">{items}</div>
        </aside>
    }
}

const SITE_SCRIPT: &str = r##"<script>
(function () {
  "use strict";
  var path = location.pathname;
  if (path.length > 1) path = path.replace(/\/+$/, "");
  function clean(href) {
    var value = new URL(href, location.href).pathname;
    if (value.length > 1) value = value.replace(/\/+$/, "");
    return value;
  }
  var links = document.querySelectorAll(".site-header nav a[href]");
  for (var index = 0; index < links.length; index += 1) {
    var link = links[index];
    var url = new URL(link.href, location.href);
    if (url.origin !== location.origin) continue;
    var href = clean(link.href);
    if (href === path) link.setAttribute("aria-current", "page");
    else if (href !== "" && path.indexOf(href + "/") === 0) link.setAttribute("aria-current", "true");
  }
  var sidebar = document.querySelectorAll(".docs-sidebar a[href]");
  for (var item = 0; item < sidebar.length; item += 1) {
    if (clean(sidebar[item].href) === path) sidebar[item].setAttribute("aria-current", "page");
  }
  var heading = document.querySelector("#main h1");
  if (heading) document.title = heading.textContent.trim() + " — Rusty";
  var lead = document.querySelector("#main .lead");
  var description = document.querySelector('meta[name="description"]');
  if (lead && description) description.setAttribute("content", lead.textContent.trim());
  var canonical = document.getElementById("rusty-canonical");
  if (!canonical) {
    canonical = document.createElement("link");
    canonical.id = "rusty-canonical";
    canonical.rel = "canonical";
    document.head.appendChild(canonical);
  }
  canonical.href = location.origin + (path || "/");
})();
</script>"##;
