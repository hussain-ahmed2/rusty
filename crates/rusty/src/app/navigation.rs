//! Client-navigation manifest and browser runtime.

use rusty_router::{RoutePattern, RouteSegment};
use rusty_server::ServerError;
use rusty_ssr::render_to_string;

use crate::app::PageRoute;

/// Builds the JSON payload embedded in each server-rendered page.
pub(crate) fn build(routes: &[PageRoute]) -> Result<String, ServerError> {
    let mut entries = Vec::new();
    for route in routes {
        let Some(loading) = route.loading else {
            continue;
        };
        let pattern = RoutePattern::parse(&route.pattern)?;
        let loading = render_to_string(&(loading)())
            .map_err(|error| ServerError::Handler(error.to_string()))?;
        entries.push((specificity(&pattern), regex(&pattern), loading));
    }

    entries.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    let routes = entries
        .iter()
        .map(|(_, source, loading)| {
            format!(
                "{{\"source\":{},\"loading\":{}}}",
                json_string(source),
                json_string(loading)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("{{\"routes\":[{routes}]}}"))
}

/// Returns the navigation scripts for a non-empty manifest.
pub(crate) fn markup(data: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    format!(
        "<script type=\"application/json\" id=\"__rusty_navigation_data\">{data}</script>{}",
        SCRIPT
    )
}

fn specificity(pattern: &RoutePattern) -> Vec<u8> {
    pattern
        .segments()
        .iter()
        .map(|segment| match segment {
            RouteSegment::Static { .. } => 2,
            RouteSegment::Dynamic { .. } => 1,
            RouteSegment::CatchAll { .. } => 0,
        })
        .collect()
}

fn regex(pattern: &RoutePattern) -> String {
    let mut source = String::from("^/");
    for (index, segment) in pattern.segments().iter().enumerate() {
        if index > 0 {
            source.push('/');
        }
        match segment {
            RouteSegment::Static { value } => push_regex_literal(&mut source, value),
            RouteSegment::Dynamic { .. } => source.push_str("[^/]+"),
            RouteSegment::CatchAll { .. } => source.push_str(".+"),
        }
    }
    source.push('$');
    source
}

fn push_regex_literal(output: &mut String, value: &str) {
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '<' => output.push_str("\\u003c"),
            '>' => output.push_str("\\u003e"),
            '&' => output.push_str("\\u0026"),
            character if character <= '\u{1f}' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

const SCRIPT: &str = r#"<script>
(function () {
  "use strict";
  var data = document.getElementById("__rusty_navigation_data");
  if (!data) return;
  var routes = [];
  try {
    routes = JSON.parse(data.textContent).routes.map(function (route) {
      route.regex = new RegExp(route.source);
      return route;
    });
  } catch (_) {}

  function routePath(pathname) {
    var base = document.querySelector("base[href]");
    if (base) {
      var basePath = new URL(base.href, location.href).pathname;
      if (basePath.length > 1) {
        if (basePath.charAt(basePath.length - 1) !== "/") basePath += "/";
        if (pathname + "/" === basePath) {
          pathname = "/";
        } else if (pathname.indexOf(basePath) === 0) {
          pathname = "/" + pathname.slice(basePath.length);
        }
      }
    }
    if (pathname.length > 1 && pathname.charAt(pathname.length - 1) === "/") {
      pathname = pathname.slice(0, -1);
    }
    return pathname || "/";
  }

  function loadingFor(pathname) {
    pathname = routePath(pathname);
    for (var index = 0; index < routes.length; index += 1) {
      if (routes[index].regex.test(pathname)) return routes[index].loading;
    }
    return "";
  }

  function overlay(html) {
    var element = document.createElement("div");
    element.id = "__rusty_navigation_loading";
    element.setAttribute("role", "status");
    element.setAttribute("aria-live", "polite");
    element.setAttribute(
      "style",
      "position:fixed;inset:0;z-index:2147483647;overflow:auto;background:Canvas;color:CanvasText"
    );
    element.innerHTML = html || '<div style="padding:1rem;font:14px system-ui,sans-serif">Loading&hellip;</div>';
    document.documentElement.appendChild(element);
    return element;
  }

  function navigate(url, push) {
    if (!window.__rustyNavigation) return;
    if (!window.__rustyNavigation.pending) {
      window.__rustyNavigation.pending = true;
      var current = overlay(loadingFor(new URL(url).pathname));
      var controller = new AbortController();
      var timeout = setTimeout(function () { controller.abort(); }, 20000);
      fetch(url, {
        credentials: "same-origin",
        headers: { "Accept": "text/html", "X-Rusty-Navigation": "1" },
        signal: controller.signal
      })
        .then(function (response) {
          var type = response.headers.get("content-type") || "";
          if (!response.ok || type.indexOf("text/html") === -1) {
            throw new Error("navigation failed");
          }
          return response.text().then(function (html) {
            return { html: html, url: response.url || url };
          });
        })
        .then(function (result) {
          clearTimeout(timeout);
          if (push) history.pushState({ rusty: true }, "", result.url);
          document.open();
          document.write(result.html);
          document.close();
        })
        .catch(function () {
          clearTimeout(timeout);
          window.__rustyNavigation.pending = false;
          current.remove();
          location.assign(url);
        });
    }
  }

  window.__rustyNavigation = { pending: false, navigate: navigate };
  window.addEventListener("popstate", function () {
    if (window.__rustyNavigation) {
      window.__rustyNavigation.navigate(location.href, false);
    }
  });

  document.addEventListener("click", function (event) {
    if (event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    var link = event.target.closest("a[href]");
    if (!link) return;
    if ((link.target && link.target !== "_self") || link.hasAttribute("download")) return;
    if (link.hasAttribute("data-rusty-reload")) return;
    if (link.relList && link.relList.contains("external")) return;
    var url = new URL(link.href, location.href);
    if (url.origin !== location.origin) return;
    if (url.pathname === location.pathname && url.search === location.search) return;
    if (url.pathname.indexOf("/_rusty/") === 0) return;
    event.preventDefault();
    navigate(url.href, true);
  });
})();
</script>"#;
