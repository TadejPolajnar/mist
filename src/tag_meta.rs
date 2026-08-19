//! Per-tag metadata for native WeChat components, hand-curated against the
//! WeChat component docs (miniprogram-api-typings ships event payload types
//! only, not per-component attribute tables, so it cannot be generated).
//! Tags absent from the table skip validation entirely; validation is
//! warning-tier (M1023/M1024) and suppressible via `config.customAttrs`.

pub struct TagMeta {
    pub tag: &'static str,
    pub attrs: &'static [&'static str],
    pub events: &'static [&'static str],
}

pub const COMMON_EVENTS: &[&str] = &[
    "Tap",
    "LongPress",
    "LongTap",
    "TouchStart",
    "TouchMove",
    "TouchCancel",
    "TouchEnd",
    "TouchForceChange",
    "TransitionEnd",
    "AnimationStart",
    "AnimationIteration",
    "AnimationEnd",
];

pub const UNIVERSAL_ATTRS: &[&str] = &[
    "class",
    "style",
    "id",
    "hidden",
    "slot",
    "key",
    "hover-class",
    "hover-stop-propagation",
    "hover-start-time",
    "hover-stay-time",
    "animation",
];

pub const TAG_META: &[TagMeta] = &[
    TagMeta { tag: "view", attrs: &[], events: &[] },
    TagMeta {
        tag: "text",
        attrs: &["selectable", "user-select", "space", "decode", "overflow", "max-lines"],
        events: &[],
    },
    TagMeta {
        tag: "image",
        attrs: &["src", "mode", "webp", "lazy-load", "show-menu-by-longpress", "fade-in"],
        events: &["Load", "Error"],
    },
    TagMeta {
        tag: "button",
        attrs: &[
            "size",
            "type",
            "plain",
            "disabled",
            "loading",
            "form-type",
            "open-type",
            "lang",
            "session-from",
            "send-message-title",
            "send-message-path",
            "send-message-img",
            "app-parameter",
            "show-message-card",
            "phone-number-no-quota-toast",
            "need-show-entrance",
            "entrance-path",
        ],
        events: &[
            "GetUserInfo",
            "Contact",
            "GetPhoneNumber",
            "GetRealtimePhoneNumber",
            "Error",
            "OpenSetting",
            "LaunchApp",
            "ChooseAvatar",
            "AgreePrivacyAuthorization",
        ],
    },
    TagMeta {
        tag: "input",
        attrs: &[
            "value",
            "type",
            "password",
            "placeholder",
            "placeholder-style",
            "placeholder-class",
            "disabled",
            "maxlength",
            "cursor-spacing",
            "auto-focus",
            "focus",
            "confirm-type",
            "always-embed",
            "confirm-hold",
            "cursor",
            "cursor-color",
            "selection-start",
            "selection-end",
            "adjust-position",
            "hold-keyboard",
        ],
        events: &[
            "Input",
            "Change",
            "Focus",
            "Blur",
            "Confirm",
            "KeyboardHeightChange",
            "NicknameReview",
            "SelectionChange",
            "KeyboardCompositionStart",
            "KeyboardCompositionUpdate",
            "KeyboardCompositionEnd",
        ],
    },
    TagMeta {
        tag: "textarea",
        attrs: &[
            "value",
            "placeholder",
            "placeholder-style",
            "placeholder-class",
            "disabled",
            "maxlength",
            "auto-focus",
            "focus",
            "auto-height",
            "fixed",
            "cursor-spacing",
            "cursor",
            "show-confirm-bar",
            "selection-start",
            "selection-end",
            "adjust-position",
            "hold-keyboard",
            "disable-default-padding",
            "confirm-type",
            "confirm-hold",
            "adjust-keyboard-to",
        ],
        events: &[
            "Focus",
            "Blur",
            "LineChange",
            "Input",
            "Confirm",
            "KeyboardHeightChange",
            "SelectionChange",
            "KeyboardCompositionStart",
            "KeyboardCompositionUpdate",
            "KeyboardCompositionEnd",
        ],
    },
    TagMeta {
        tag: "scroll-view",
        attrs: &[
            "scroll-x",
            "scroll-y",
            "upper-threshold",
            "lower-threshold",
            "scroll-top",
            "scroll-left",
            "scroll-into-view",
            "scroll-with-animation",
            "enable-back-to-top",
            "enable-flex",
            "scroll-anchoring",
            "refresher-enabled",
            "refresher-threshold",
            "refresher-default-style",
            "refresher-background",
            "refresher-triggered",
            "enable-passive",
            "using-sticky",
            "show-scrollbar",
            "fast-deceleration",
            "type",
            "associative-container",
            "reverse",
            "clip",
            "bounces",
            "enhanced",
            "paging-enabled",
            "scroll-into-view-offset",
        ],
        events: &[
            "ScrollToUpper",
            "ScrollToLower",
            "Scroll",
            "ScrollStart",
            "ScrollEnd",
            "RefresherPulling",
            "RefresherRefresh",
            "RefresherRestore",
            "RefresherAbort",
            "RefresherWillRefresh",
            "DragStart",
            "Dragging",
            "DragEnd",
        ],
    },
    TagMeta {
        tag: "swiper",
        attrs: &[
            "indicator-dots",
            "indicator-color",
            "indicator-active-color",
            "autoplay",
            "current",
            "interval",
            "duration",
            "circular",
            "vertical",
            "previous-margin",
            "next-margin",
            "display-multiple-items",
            "easing-function",
            "snap-to-edge",
            "direction",
        ],
        events: &["Change", "Transition", "AnimationFinish"],
    },
    TagMeta {
        tag: "swiper-item",
        attrs: &["item-id", "skip-hidden-item-layout"],
        events: &[],
    },
    TagMeta {
        tag: "picker",
        attrs: &[
            "mode",
            "disabled",
            "value",
            "range",
            "range-key",
            "start",
            "end",
            "fields",
            "custom-item",
            "header-text",
            "level",
        ],
        events: &["Change", "Cancel", "ColumnChange"],
    },
    TagMeta {
        tag: "picker-view",
        attrs: &[
            "value",
            "indicator-style",
            "indicator-class",
            "mask-style",
            "mask-class",
            "immediate-change",
        ],
        events: &["Change", "PickStart", "PickEnd"],
    },
    TagMeta { tag: "picker-view-column", attrs: &[], events: &[] },
    TagMeta {
        tag: "switch",
        attrs: &["checked", "disabled", "type", "color"],
        events: &["Change"],
    },
    TagMeta {
        tag: "slider",
        attrs: &[
            "min",
            "max",
            "step",
            "disabled",
            "value",
            "color",
            "selected-color",
            "activeColor",
            "backgroundColor",
            "block-size",
            "block-color",
            "show-value",
        ],
        events: &["Change", "Changing"],
    },
    TagMeta {
        tag: "checkbox",
        attrs: &["value", "disabled", "checked", "color"],
        events: &[],
    },
    TagMeta { tag: "checkbox-group", attrs: &[], events: &["Change"] },
    TagMeta {
        tag: "radio",
        attrs: &["value", "checked", "disabled", "color"],
        events: &[],
    },
    TagMeta { tag: "radio-group", attrs: &[], events: &["Change"] },
    TagMeta {
        tag: "form",
        attrs: &["report-submit", "report-submit-timeout"],
        events: &["Submit", "Reset", "SubmitToGroup"],
    },
    TagMeta { tag: "label", attrs: &["for"], events: &[] },
    TagMeta {
        tag: "navigator",
        attrs: &[
            "target",
            "url",
            "open-type",
            "delta",
            "app-id",
            "path",
            "extra-data",
            "version",
            "short-link",
        ],
        events: &["Success", "Fail", "Complete"],
    },
    TagMeta {
        tag: "progress",
        attrs: &[
            "percent",
            "show-info",
            "border-radius",
            "font-size",
            "stroke-width",
            "color",
            "activeColor",
            "backgroundColor",
            "active",
            "active-mode",
            "duration",
        ],
        events: &["ActiveEnd"],
    },
    TagMeta { tag: "icon", attrs: &["type", "size", "color"], events: &[] },
    TagMeta {
        tag: "rich-text",
        attrs: &["nodes", "space", "user-select", "mode"],
        events: &[],
    },
    TagMeta {
        tag: "video",
        attrs: &[
            "src",
            "duration",
            "controls",
            "danmu-list",
            "danmu-btn",
            "enable-danmu",
            "autoplay",
            "loop",
            "muted",
            "initial-time",
            "page-gesture",
            "direction",
            "show-progress",
            "show-fullscreen-btn",
            "show-play-btn",
            "show-center-play-btn",
            "enable-progress-gesture",
            "object-fit",
            "poster",
            "show-mute-btn",
            "title",
            "play-btn-position",
            "enable-play-gesture",
            "auto-pause-if-navigate",
            "auto-pause-if-open-native",
            "vslide-gesture",
            "vslide-gesture-in-fullscreen",
            "show-bottom-progress",
            "ad-unit-id",
            "poster-for-crawler",
            "show-casting-button",
            "picture-in-picture-mode",
            "picture-in-picture-show-progress",
            "enable-auto-rotation",
            "show-screen-lock-button",
            "show-snapshot-button",
            "show-background-playback-button",
            "background-poster",
            "referrer-policy",
        ],
        events: &[
            "Play",
            "Pause",
            "Ended",
            "TimeUpdate",
            "FullScreenChange",
            "Waiting",
            "Error",
            "Progress",
            "LoadedMetaData",
            "ControlsToggle",
            "EnterPictureInPicture",
            "LeavePictureInPicture",
            "SeekComplete",
        ],
    },
];

pub fn meta_for(tag: &str) -> Option<&'static TagMeta> {
    TAG_META.iter().find(|m| m.tag == tag)
}

pub fn valid_event(meta: &TagMeta, event_lower: &str) -> bool {
    COMMON_EVENTS
        .iter()
        .chain(meta.events.iter())
        .any(|e| e.to_lowercase() == event_lower)
}

pub fn suggest_event(meta: &TagMeta, event_lower: &str) -> Option<String> {
    COMMON_EVENTS
        .iter()
        .chain(meta.events.iter())
        .map(|e| (e, crate::wxml::levenshtein(&e.to_lowercase(), event_lower)))
        .filter(|(_, d)| *d <= 2)
        .min_by_key(|(_, d)| *d)
        .map(|(e, _)| format!("on{}", e))
}

pub fn valid_attr(meta: &TagMeta, attr: &str) -> bool {
    UNIVERSAL_ATTRS.contains(&attr) || meta.attrs.contains(&attr)
}

pub fn suggest_attr(meta: &TagMeta, attr: &str) -> Option<String> {
    UNIVERSAL_ATTRS
        .iter()
        .chain(meta.attrs.iter())
        .map(|a| (a, crate::wxml::levenshtein(a, attr)))
        .filter(|(_, d)| *d <= 2)
        .min_by_key(|(_, d)| *d)
        .map(|(a, _)| a.to_string())
}

/// Documented WeChat base-library minimums for table features — curated,
/// deliberately incomplete: absence means "no version check", never an error.
pub const SINCE: &[(&str, &str, &str)] = &[
    ("input", "value:bind", "2.9.3"),
    ("textarea", "value:bind", "2.9.3"),
    ("switch", "checked:bind", "2.9.3"),
    ("checkbox", "checked:bind", "2.9.3"),
    ("scroll-view", "refresher-enabled", "2.10.1"),
    ("scroll-view", "refresher-threshold", "2.10.1"),
    ("scroll-view", "refresher-default-style", "2.10.1"),
    ("scroll-view", "refresher-background", "2.10.1"),
    ("scroll-view", "refresher-triggered", "2.10.1"),
    ("scroll-view", "onRefresherPulling", "2.10.1"),
    ("scroll-view", "onRefresherRefresh", "2.10.1"),
    ("scroll-view", "onRefresherRestore", "2.10.1"),
    ("scroll-view", "onRefresherAbort", "2.10.1"),
    ("scroll-view", "enhanced", "2.12.0"),
    ("scroll-view", "bounces", "2.12.0"),
    ("scroll-view", "show-scrollbar", "2.12.0"),
    ("scroll-view", "paging-enabled", "2.12.0"),
    ("scroll-view", "fast-deceleration", "2.12.0"),
    ("scroll-view", "onDragStart", "2.12.0"),
    ("scroll-view", "onDragging", "2.12.0"),
    ("scroll-view", "onDragEnd", "2.12.0"),
    ("text", "user-select", "2.12.1"),
    ("image", "webp", "2.9.0"),
    ("image", "show-menu-by-longpress", "2.7.0"),
    ("swiper", "easing-function", "2.6.5"),
    ("swiper", "snap-to-edge", "2.12.1"),
    ("input", "onKeyboardHeightChange", "2.7.0"),
    ("textarea", "onKeyboardHeightChange", "2.7.0"),
    ("input", "onNicknameReview", "2.29.1"),
    ("button", "onChooseAvatar", "2.21.2"),
    ("button", "onGetRealtimePhoneNumber", "2.24.4"),
    ("button", "onAgreePrivacyAuthorization", "2.32.3"),
];

pub fn since_of(tag: &str, name: &str) -> Option<&'static str> {
    SINCE.iter().find(|(t, n, _)| *t == tag && *n == name).map(|(_, _, v)| *v)
}

/// `a < b` for dotted numeric versions ("2.9.3" < "2.10.1").
pub fn version_lt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (va, vb) = (parse(a), parse(b));
    for i in 0..va.len().max(vb.len()) {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x != y {
            return x < y;
        }
    }
    false
}
