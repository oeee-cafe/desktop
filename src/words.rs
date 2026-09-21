//! The few sentences the app says itself, in the languages the site speaks.
//!
//! Dialogs are the app's, not the page's, so they follow the system language
//! as every other native dialog on the machine does.

pub struct Words {
    pub ok: &'static str,
    pub cancel: &'static str,
    pub leave_title: &'static str,
    pub leave_body: &'static str,
    pub leave: &'static str,
    pub stay: &'static str,
}

const EN: Words = Words {
    ok: "OK",
    cancel: "Cancel",
    leave_title: "Leave this page?",
    leave_body: "Anything you have not saved will be lost.",
    leave: "Leave",
    stay: "Stay",
};

const KO: Words = Words {
    ok: "확인",
    cancel: "취소",
    leave_title: "이 페이지를 떠날까요?",
    leave_body: "저장하지 않은 내용은 사라집니다.",
    leave: "떠나기",
    stay: "머무르기",
};

const JA: Words = Words {
    ok: "OK",
    cancel: "キャンセル",
    leave_title: "このページを離れますか？",
    leave_body: "保存していない内容は失われます。",
    leave: "離れる",
    stay: "とどまる",
};

const ZH: Words = Words {
    ok: "确定",
    cancel: "取消",
    leave_title: "要离开此页面吗？",
    leave_body: "未保存的内容将会丢失。",
    leave: "离开",
    stay: "留下",
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
