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

  if ("scrollRestoration" in history) history.scrollRestoration = "manual";

  function scrollPosition() {
    return window.scrollY || window.pageYOffset || 0;
  }

  function rememberScroll() {
    if (window.__rustyNavigation && window.__rustyNavigation.pending) return;
    var state = history.state;
    if (!state || !state.rusty) state = { rusty: true };
    state.scrollY = scrollPosition();
    history.replaceState(state, "", location.href);
  }

  if (!history.state || !history.state.rusty) {
    history.replaceState({ rusty: true, scrollY: scrollPosition() }, "", location.href);
  }

  var scrollTimer = 0;
  window.addEventListener("scroll", function () {
    if (scrollTimer) return;
    scrollTimer = setTimeout(function () {
      scrollTimer = 0;
      rememberScroll();
    }, 100);
  }, { passive: true });

  function scrollAfterLoad(hash, position) {
    requestAnimationFrame(function () {
      var root = document.documentElement;
      var behavior = root.style.scrollBehavior;
      root.style.scrollBehavior = "auto";
      if (hash) {
        var target = document.getElementById(hash.slice(1));
        if (target) {
          target.scrollIntoView();
          root.style.scrollBehavior = behavior;
          return;
        }
      }
      window.scrollTo(0, position);
      root.style.scrollBehavior = behavior;
    });
  }

  function routePath(pathname) {
    var meta = document.querySelector('meta[name="rusty-base"]');
    var base = document.querySelector("base[href]");
    var baseHref = meta ? meta.getAttribute("content") : base ? base.href : null;
    if (baseHref) {
      var basePath = new URL(baseHref, location.href).pathname;
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
      "position:fixed;inset:0;z-index:2147483647;overflow-y:auto;overflow-x:hidden;overscroll-behavior:contain;background:Canvas;color:CanvasText"
    );
    element.innerHTML = html || '<div style="padding:1rem;font:14px system-ui,sans-serif">Loading&hellip;</div>';
    (document.body || document.documentElement).appendChild(element);
    return element;
  }

  function navigate(url, push) {
    if (!window.__rustyNavigation) return;
    if (!window.__rustyNavigation.pending) {
      if (push) rememberScroll();
      window.__rustyNavigation.pending = true;
      var current = overlay(loadingFor(new URL(url).pathname));
      var root = document.documentElement;
      var previousOverflow = root.style.overflow;
      var targetScroll = push
        ? 0
        : history.state && typeof history.state.scrollY === "number"
          ? history.state.scrollY
          : 0;
      root.style.overflow = "hidden";
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
          var hash = push ? new URL(result.url).hash : "";
          if (push) history.pushState({ rusty: true, scrollY: 0 }, "", result.url);
          document.open();
          document.write(result.html);
          document.close();
          scrollAfterLoad(hash, targetScroll);
        })
        .catch(function () {
          clearTimeout(timeout);
          window.__rustyNavigation.pending = false;
          current.remove();
          root.style.overflow = previousOverflow;
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
