//! Tool schema definitions exposed to the agent via MCP-lite `tools.list`.
//!
//! [`tool_definitions`] returns the complete list registered in `main`.  Each
//! entry maps directly to one `handle_*` function in the `handlers` module.

use sdk_rust::ToolDefinition;
use serde_json::json;

/// Return all browser tool definitions in MCP-lite format.
#[allow(
    clippy::too_many_lines,
    reason = "flat list of tool schemas — splitting adds no clarity"
)]
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // ── Session lifecycle ────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.open".into(),
            description: "Open a URL in a new or existing named browser session. Each session has isolated cookies/storage. Returns session_id and screenshot path. Pass session_id to reuse an existing session. The service applies default browser identity settings from openagent config.".into(),
            params: json!({ "type":"object","properties":{ "url":{"type":"string","description":"URL to open"},"session_id":{"type":"string","description":"Optional: reuse existing session"} },"required":["url"] }),
        },
        ToolDefinition {
            name: "browser.navigate".into(),
            description: "Navigate an existing session to a new URL.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"url":{"type":"string"} },"required":["session_id","url"] }),
        },
        ToolDefinition {
            name: "browser.close".into(),
            description: "Close this browser session and release resources.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        // ── Page observation ─────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.snapshot".into(),
            description: "Get accessibility tree (AI-optimized page representation) + screenshot. Ref IDs like @e1, @e2 can be used in click/fill/type.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"interactive_only":{"type":"boolean","description":"Only include interactive elements (default false)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.screenshot".into(),
            description: "Take a screenshot of the current page.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"full_page":{"type":"boolean","description":"Full page scroll screenshot (default false)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.get".into(),
            description: "Get data from the page: text, html, value, attr, title, url, count, box.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"what":{"type":"string","enum":["text","html","value","attr","title","url","count","box","styles"],"description":"What to get"},"selector":{"type":"string","description":"CSS selector (required for text/html/value/attr/count/box)"},"attr":{"type":"string","description":"Attribute name (for what=attr)"} },"required":["session_id","what"] }),
        },
        ToolDefinition {
            name: "browser.wait".into(),
            description: "Wait for a condition: element visible, text to appear, URL pattern, load state, or a delay in ms.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"Wait for element to be visible"},"text":{"type":"string","description":"Wait for text to appear"},"url_pattern":{"type":"string","description":"Wait for URL to match pattern"},"load_state":{"type":"string","enum":["load","domcontentloaded","networkidle"]},"ms":{"type":"number","description":"Wait N milliseconds"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.eval".into(),
            description: "Evaluate JavaScript in the page context and return the result.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"js":{"type":"string","description":"JavaScript expression to evaluate"} },"required":["session_id","js"] }),
        },
        ToolDefinition {
            name: "browser.extract".into(),
            description: "Extract readable text content from the page or a CSS-scoped element.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"Scope extraction to this CSS selector (optional)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.is".into(),
            description: "Check element state: visible, enabled, or checked. Returns boolean result.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"check":{"type":"string","enum":["visible","enabled","checked"],"description":"What to check (default: visible)"},"selector":{"type":"string","description":"CSS selector or @ref"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.console".into(),
            description: "View browser console messages (log, warn, error). Set clear=true to clear.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"clear":{"type":"boolean","description":"Clear console after reading (default: false)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.errors".into(),
            description: "View uncaught JavaScript errors on the page. Set clear=true to clear.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"clear":{"type":"boolean","description":"Clear errors after reading (default: false)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.diff".into(),
            description: "Diff current page snapshot or screenshot against the previous or a baseline file.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"kind":{"type":"string","enum":["snapshot","screenshot"],"description":"What to diff (default: snapshot)"},"baseline":{"type":"string","description":"Baseline file path (for screenshot diff)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.highlight".into(),
            description: "Visually highlight an element on the page (useful for verifying selectors).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref"} },"required":["session_id","selector"] }),
        },
        // ── User interaction ─────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.click".into(),
            description: "Click an element by CSS selector, accessibility ref (@e1), or pixel coordinates x+y.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref (e.g. @e2)"},"x":{"type":"number"},"y":{"type":"number"},"new_tab":{"type":"boolean"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.dblclick".into(),
            description: "Double-click an element by CSS selector or accessibility ref.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.fill".into(),
            description: "Clear an input and fill with text. Preferred over type for form inputs.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref"},"text":{"type":"string"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.type".into(),
            description: "Type text into a selector (appends to existing value) or use keyboard type if no selector.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"text":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref (optional)"} },"required":["session_id","text"] }),
        },
        ToolDefinition {
            name: "browser.press".into(),
            description: "Press a key or key combination. Examples: Enter, Tab, Control+a, Escape, ArrowDown.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"key":{"type":"string","description":"Key name e.g. Enter, Tab, Control+a"} },"required":["session_id","key"] }),
        },
        ToolDefinition {
            name: "browser.hover".into(),
            description: "Hover over an element (triggers tooltips, dropdown menus, etc.).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.select".into(),
            description: "Select an option in a <select> dropdown.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for <select>"},"value":{"type":"string","description":"Option value to select"} },"required":["session_id","selector","value"] }),
        },
        ToolDefinition {
            name: "browser.check".into(),
            description: "Check or uncheck a checkbox. Set uncheck=true to uncheck.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string"},"uncheck":{"type":"boolean","description":"Set true to uncheck (default: check)"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.scroll".into(),
            description: "Scroll the page or a specific element.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"direction":{"type":"string","enum":["up","down","left","right"]},"amount":{"type":"number","description":"Pixels (default 500)"},"selector":{"type":"string","description":"Scroll inside this element (optional)"} },"required":["session_id","direction"] }),
        },
        ToolDefinition {
            name: "browser.scrollinto".into(),
            description: "Scroll an element into view.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.find".into(),
            description: "Find and interact with elements by semantic attributes: role, text, label, placeholder, alt, testid.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"by":{"type":"string","enum":["role","text","label","placeholder","alt","testid","title"],"description":"How to find element"},"value":{"type":"string","description":"Value to match"},"action":{"type":"string","description":"Action: click, fill, check (default: click)"},"action_value":{"type":"string","description":"Value for fill action"} },"required":["session_id","by","value"] }),
        },
        ToolDefinition {
            name: "browser.focus".into(),
            description: "Focus an element without clicking it. Useful before keyboard shortcuts.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref"} },"required":["session_id","selector"] }),
        },
        ToolDefinition {
            name: "browser.drag".into(),
            description: "Drag an element from source to target (CSS selectors or @refs).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"source":{"type":"string","description":"CSS selector or @ref to drag from"},"target":{"type":"string","description":"CSS selector or @ref to drop onto"} },"required":["session_id","source","target"] }),
        },
        ToolDefinition {
            name: "browser.upload".into(),
            description: "Upload a file to a file input element.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector or @ref for file input"},"file":{"type":"string","description":"Absolute path to the file to upload"} },"required":["session_id","selector","file"] }),
        },
        ToolDefinition {
            name: "browser.keydown".into(),
            description: "Hold a key down (pair with browser.keyup). Use for Shift+click, drag-with-modifier, etc.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"key":{"type":"string","description":"Key to hold: Shift, Control, Alt, Meta"} },"required":["session_id","key"] }),
        },
        ToolDefinition {
            name: "browser.keyup".into(),
            description: "Release a held key (use after browser.keydown).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"key":{"type":"string","description":"Key to release: Shift, Control, Alt, Meta"} },"required":["session_id","key"] }),
        },
        // ── Navigation history ───────────────────────────────────────────────
        ToolDefinition {
            name: "browser.back".into(),
            description: "Navigate back to the previous page.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.forward".into(),
            description: "Navigate forward to the next page.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.reload".into(),
            description: "Reload the current page.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        // ── Tabs ─────────────────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.tab_new".into(),
            description: "Open a new tab in this session, optionally navigating to a URL.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"url":{"type":"string"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.tab_switch".into(),
            description: "Switch to tab N (1-indexed) in this session.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"n":{"type":"number","description":"Tab number (1-indexed)"} },"required":["session_id","n"] }),
        },
        ToolDefinition {
            name: "browser.tab_list".into(),
            description: "List all open tabs in this session.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.tab_close".into(),
            description: "Close current tab or a specific tab by number (1-indexed).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"n":{"type":"number","description":"Tab number to close (default: current tab)"} },"required":["session_id"] }),
        },
        // ── Frames & dialogs ─────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.frame".into(),
            description: "Switch into an iframe (selector) or back to the main frame (selector='main').".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for iframe, or 'main' to return to top frame (default: main)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.dialog".into(),
            description: "Accept or dismiss a browser alert/confirm/prompt dialog.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"action":{"type":"string","enum":["accept","dismiss"],"description":"accept or dismiss (default: dismiss)"},"text":{"type":"string","description":"Text to enter for prompt dialogs (accept only)"} },"required":["session_id"] }),
        },
        // ── Storage & state ──────────────────────────────────────────────────
        ToolDefinition {
            name: "browser.cookies".into(),
            description: "Get, set, or clear cookies for this session.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"action":{"type":"string","enum":["get","set","clear"],"description":"Action (default: get)"},"name":{"type":"string","description":"Cookie name (for set)"},"value":{"type":"string","description":"Cookie value (for set)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.state".into(),
            description: "Save or load auth state (cookies, localStorage) to/from disk for this session.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"action":{"type":"string","enum":["save","load"],"description":"save or load (default: save)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.storage".into(),
            description: "Read or write localStorage/sessionStorage. Actions: get (all), get key, set key+value, clear.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"store":{"type":"string","enum":["local","session"],"description":"local=localStorage, session=sessionStorage (default: local)"},"action":{"type":"string","enum":["get","set","clear"],"description":"get, set, or clear (default: get)"},"key":{"type":"string","description":"Storage key (get specific key or set)"},"value":{"type":"string","description":"Value to store (for set)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.pdf".into(),
            description: "Save the current page as a PDF to the session artifacts directory.".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"} },"required":["session_id"] }),
        },
        // ── Browser configuration ────────────────────────────────────────────
        ToolDefinition {
            name: "browser.set".into(),
            description: "Configure browser settings. what=viewport(width,height), device(name), geo(lat,lng), offline(value=on/off), headers(json), credentials(username,password), media(scheme=dark/light).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"what":{"type":"string","enum":["viewport","device","geo","offline","headers","credentials","media"],"description":"What setting to change"},"width":{"type":"number"},"height":{"type":"number"},"name":{"type":"string"},"lat":{"type":"number"},"lng":{"type":"number"},"value":{"type":"string"},"json":{"type":"string"},"username":{"type":"string"},"password":{"type":"string"},"scheme":{"type":"string","enum":["dark","light"]} },"required":["session_id","what"] }),
        },
        ToolDefinition {
            name: "browser.network".into(),
            description: "Intercept or inspect network requests. action=route(url_pattern, abort=true or body=json), unroute(url_pattern), requests(filter).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"action":{"type":"string","enum":["route","unroute","requests"],"description":"route, unroute, or requests (default: requests)"},"url_pattern":{"type":"string","description":"URL glob pattern (for route/unroute)"},"abort":{"type":"boolean","description":"Block matching requests (route only)"},"body":{"type":"string","description":"Mock response JSON (route only)"},"filter":{"type":"string","description":"Filter pattern (requests only)"} },"required":["session_id"] }),
        },
        ToolDefinition {
            name: "browser.mouse".into(),
            description: "Low-level mouse control: move(x,y), down(button), up(button), wheel(dy,dx).".into(),
            params: json!({ "type":"object","properties":{ "session_id":{"type":"string"},"action":{"type":"string","enum":["move","down","up","wheel"],"description":"Mouse action"},"x":{"type":"number"},"y":{"type":"number"},"button":{"type":"string","enum":["left","right","middle"],"description":"Mouse button (default: left)"},"dy":{"type":"number","description":"Vertical wheel delta"},"dx":{"type":"number","description":"Horizontal wheel delta"} },"required":["session_id","action"] }),
        },
    ]
}
