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
            <div class="skeleton skeleton-search"></div>
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
  function setMeta(selector, value) {
    var node = document.querySelector(selector);
    if (node) node.setAttribute("content", value);
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
  var search = document.getElementById("docs-search");
  if (search) {
    var navItems = document.querySelectorAll(".docs-sidebar li");
    var cards = document.querySelectorAll(".grid .link-card");
    var empty = document.getElementById("docs-search-empty");
    var filter = function () {
      var query = search.value.trim().toLowerCase();
      var matches = 0;
      var navMatches = 0;
      for (var nav = 0; nav < navItems.length; nav += 1) {
        var navText = (navItems[nav].textContent || "").toLowerCase();
        var showNav = query === "" || navText.indexOf(query) !== -1;
        navItems[nav].hidden = !showNav;
        if (showNav) navMatches += 1;
      }
      for (var card = 0; card < cards.length; card += 1) {
        var cardText = (cards[card].textContent || "").toLowerCase();
        var show = query === "" || cardText.indexOf(query) !== -1;
        cards[card].hidden = !show;
        if (show) matches += 1;
      }
      var found = cards.length > 0 ? matches > 0 : navMatches > 0;
      if (empty) empty.hidden = query === "" || found;
    };
    var params = new URLSearchParams(location.search);
    if (params.get("q")) search.value = params.get("q");
    search.addEventListener("input", function () {
      filter();
      var url = new URL(location.href);
      var query = search.value.trim();
      if (query === "") url.searchParams.delete("q");
      else url.searchParams.set("q", query);
      history.replaceState(null, "", url);
    });
    filter();
  }
  var heading = document.querySelector("#main h1");
  if (heading) document.title = heading.textContent.trim() + " — Rusty";
  var lead = document.querySelector("#main .lead");
  var description = document.querySelector('meta[name="description"]');
  if (lead && description) description.setAttribute("content", lead.textContent.trim());
  var pageUrl = location.origin + (path || "/");
  var pageTitle = document.title;
  var pageDescription = description ? description.getAttribute("content") : "";
  setMeta('meta[property="og:title"]', pageTitle);
  setMeta('meta[property="og:description"]', pageDescription);
  setMeta('meta[name="twitter:title"]', pageTitle);
  setMeta('meta[name="twitter:description"]', pageDescription);
  var socialUrl = document.querySelector('meta[property="og:url"]');
  if (!socialUrl) {
    socialUrl = document.createElement("meta");
    socialUrl.setAttribute("property", "og:url");
    document.head.appendChild(socialUrl);
  }
  socialUrl.setAttribute("content", pageUrl);
  var canonical = document.getElementById("rusty-canonical");
  if (!canonical) {
    canonical = document.createElement("link");
    canonical.id = "rusty-canonical";
    canonical.rel = "canonical";
    document.head.appendChild(canonical);
  }
  canonical.href = pageUrl;
})();
</script>"##;
