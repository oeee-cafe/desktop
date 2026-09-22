//! The few sentences the app says itself, in the languages the site speaks.
//!
//! Dialogs are the app's, not the page's, so they follow the system language
//! as every other native dialog on the machine does.

pub struct Words {
    /// The title over the page's own `alert()` and `confirm()`.
    pub app_name: &'static str,
    pub leave_title: &'static str,
    pub leave_body: &'static str,
    pub leave: &'static str,
    pub stay: &'static str,
    /// Steam did not hand over a sign-in ticket.
    pub steam_sign_in_failed: &'static str,
    /// The right-click menu's item for a link (webview2.rs).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub copy_link: &'static str,
}

const EN: Words = Words {
    app_name: "Oeee Cafe",
    leave_title: "Leave this page?",
    leave_body: "Anything you have not saved will be lost.",
    leave: "Leave",
    stay: "Stay",
    steam_sign_in_failed: "Steam could not sign you in. Make sure Steam is running and try again.",
    copy_link: "Copy link",
};

const KO: Words = Words {
    app_name: "오이카페",
    leave_title: "이 페이지를 떠날까요?",
    leave_body: "저장하지 않은 내용은 사라집니다.",
    leave: "떠나기",
    stay: "머무르기",
    steam_sign_in_failed: "Steam으로 로그인하지 못했습니다. Steam이 실행 중인지 확인하고 다시 시도해 주세요.",
    copy_link: "링크 복사",
};

const JA: Words = Words {
    app_name: "OEEEカフェ",
    leave_title: "このページを離れますか？",
    leave_body: "保存していない内容は失われます。",
    leave: "離れる",
    stay: "とどまる",
    steam_sign_in_failed: "Steamでログインできませんでした。Steamが起動しているか確認して、もう一度お試しください。",
    copy_link: "リンクをコピー",
};

const ZH: Words = Words {
    app_name: "黄瓜咖啡馆",
    leave_title: "要离开此页面吗？",
    leave_body: "未保存的内容将会丢失。",
    leave: "离开",
    stay: "留下",
    steam_sign_in_failed: "无法通过 Steam 登录。请确认 Steam 正在运行，然后重试。",
    copy_link: "复制链接",
};

pub fn words() -> &'static Words {
    for_language(&sys_locale::get_locale().unwrap_or_default())
}

fn for_language(locale: &str) -> &'static Words {
    match locale.get(..2) {
        Some("ko") => &KO,
        Some("ja") => &JA,
        Some("zh") => &ZH,
        _ => &EN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_picks_its_language() {
        assert_eq!(for_language("ko-KR").leave, KO.leave);
        assert_eq!(for_language("ja").leave, JA.leave);
        assert_eq!(for_language("zh-Hans-CN").leave, ZH.leave);
    }

    #[test]
    fn anything_else_is_english() {
        assert_eq!(for_language("fr-FR").leave, EN.leave);
        assert_eq!(for_language("").leave, EN.leave);
    }
}
