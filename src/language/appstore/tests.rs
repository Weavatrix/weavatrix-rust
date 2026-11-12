use super::AppStoreAdapter;
use crate::language::{LanguageAdapter, LanguageRegistry, SourceFile};

fn labels(path: &str, text: &str) -> Vec<String> {
    let extension = path.rsplit('.').next().unwrap();
    let registry = LanguageRegistry::default();
    let adapter = registry.adapter_for_extension(extension).expect(extension);
    let facts = adapter.parse(SourceFile { path, text }).expect("parse");
    facts
        .domains
        .into_iter()
        .map(|domain| domain.name)
        .collect()
}

#[test]
fn info_plist_application_exposes_bundle_version_and_platform() {
    let text = r#"<?xml version="1.0"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.example.GrantTap</string>
<key>CFBundleDisplayName</key><string>GrantTap</string>
<key>CFBundleShortVersionString</key><string>1.4.0</string>
<key>CFBundleVersion</key><string>42</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>DTPlatformName</key><string>iphoneos</string>
</dict></plist>"#;
    let names = labels("ios/GrantTap/Info.plist", text);
    assert!(
        names.contains(&"APP com.example.GrantTap".to_owned()),
        "{names:?}"
    );
    assert!(names.contains(&"DISPLAY GrantTap".to_owned()), "{names:?}");
    assert!(
        names.contains(&"VERSION 1.4.0 (42)".to_owned()),
        "{names:?}"
    );
    assert!(names.contains(&"PLATFORM iphoneos".to_owned()), "{names:?}");
}

#[test]
fn framework_plist_is_not_an_app_store_application() {
    let text = r#"<plist><dict>
<key>CFBundleIdentifier</key><string>com.example.GrantTapKit</string>
<key>CFBundlePackageType</key><string>FMWK</string>
</dict></plist>"#;
    assert!(labels("ios/GrantTapKit/Info.plist", text).is_empty());
}

#[test]
fn binary_plist_stays_unexpanded() {
    let facts = LanguageAdapter::parse(
        &AppStoreAdapter,
        SourceFile {
            path: "ios/GrantTap/GrantTap.entitlements",
            text: "bplist00",
        },
    )
    .unwrap();
    assert!(facts.domains.is_empty());
    assert_eq!(facts.diagnostics[0].code, "appstore.binary_plist");
}

#[test]
fn export_options_mark_app_store_distribution() {
    let text = r#"<plist><dict>
<key>method</key><string>app-store-connect</string>
</dict></plist>"#;
    assert_eq!(
        labels("ios/ExportOptions.plist", text),
        ["DISTRIBUTION app-store"]
    );
}

#[test]
fn entitlements_and_privacy_manifest_are_config() {
    let text = r#"<plist><dict>
<key>aps-environment</key><string>production</string>
<key>com.apple.developer.associated-domains</key><array><string>applinks:example.com</string></array>
</dict></plist>"#;
    let names = labels("ios/GrantTap/GrantTap.entitlements", text);
    assert!(
        names.contains(&"ENTITLEMENT aps-environment=production".to_owned()),
        "{names:?}"
    );
    assert!(
        names.iter().any(|name| name.contains("associated-domains")),
        "{names:?}"
    );
    let privacy = labels(
        "ios/GrantTap/PrivacyInfo.xcprivacy",
        "<plist><dict><key>NSPrivacyTracking</key><false/></dict></plist>",
    );
    assert_eq!(privacy, ["PRIVACY manifest"]);
}

#[test]
fn xcode_application_target_and_bundle_id_are_apps() {
    let text = r#"
A1 /* GrantTap */ = {
	isa = PBXNativeTarget;
	name = GrantTap;
	productName = GrantTap;
	productType = "com.apple.product-type.application";
};
A2 /* GrantTapKit */ = {
	isa = PBXNativeTarget;
	name = GrantTapKit;
	productType = "com.apple.product-type.framework";
};
PRODUCT_BUNDLE_IDENTIFIER = com.example.GrantTap;
PRODUCT_BUNDLE_IDENTIFIER = "$(PRODUCT_NAME)";
"#;
    let names = labels("ios/GrantTap.xcodeproj/project.pbxproj", text);
    assert!(names.contains(&"APP GrantTap".to_owned()), "{names:?}");
    assert!(
        names.contains(&"APP com.example.GrantTap".to_owned()),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|name| name.contains("GrantTapKit")),
        "{names:?}"
    );
}

#[test]
fn storekit_products_and_subscriptions_are_in_app_purchases() {
    let text = r#"{
      "products": [{"productID": "com.example.coins", "type": "Consumable"}],
      "subscriptionGroups": [{"subscriptions": [{"productID": "com.example.pro.monthly"}]}],
      "nonRenewingSubscriptions": [{"productID": "com.example.season"}]
    }"#;
    let names = labels("ios/Products.storekit", text);
    assert!(
        names.contains(&"IAP com.example.coins".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"IAP com.example.pro.monthly".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"IAP com.example.season".to_owned()),
        "{names:?}"
    );
}

#[test]
fn swift_main_app_entry_is_a_service() {
    let text = "@main\nstruct GrantTapApp: App {\n    var body: some Scene { WindowGroup {} }\n}\n";
    let names = labels("ios/GrantTap/GrantTapApp.swift", text);
    assert!(
        names.contains(&"APP ENTRY GrantTapApp".to_owned()),
        "{names:?}"
    );
    let plain = labels(
        "ios/GrantTap/Model.swift",
        "struct Model: Equatable { let id: String }\n",
    );
    assert!(
        !plain.iter().any(|name| name.starts_with("APP")),
        "{plain:?}"
    );
}
