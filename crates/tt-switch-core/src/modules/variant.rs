//! Trae 客户端档位（IDE 国内版 / SOLO 国内版 / IDE 国际版）
//! 的单一事实来源。
//!
//! 档位差异（app 名、bundle id、数据根、agent token 路径、快照文件集）只允许从这里取，
//! 其它模块不得再出现档位相关字面量。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::modules::config::home_dir;

// ---------------------------------------------------------------------------
// Trae IDE 国内版
// ---------------------------------------------------------------------------
const IDE_CN_MACOS_APPS: [&str; 1] = ["Trae CN.app"];
const IDE_CN_MACOS_BUNDLE_ID: &str = "cn.trae.app";
const IDE_CN_DATA_ROOT: &str = "Library/Application Support/Trae CN";

// ---------------------------------------------------------------------------
// Trae SOLO 国内版
// ---------------------------------------------------------------------------
const SOLO_CN_MACOS_APPS: [&str; 1] = ["TRAE SOLO CN.app"];
const SOLO_CN_MACOS_BUNDLE_ID: &str = "cn.trae.solo.app";
const SOLO_CN_DATA_ROOT: &str = "Library/Application Support/TRAE SOLO CN";

// ---------------------------------------------------------------------------
// Trae IDE 国际版（bundle id 待实测，先占位）
// ---------------------------------------------------------------------------
const IDE_INTL_MACOS_APPS: [&str; 1] = ["Trae.app"];
const IDE_INTL_MACOS_BUNDLE_ID: &str = "com.trae.app";
const IDE_INTL_DATA_ROOT: &str = "Library/Application Support/Trae";

// agent token（CN 两端共用 ~/.trae-cn，国际版 ~/.trae）
const CN_AGENT_TOKEN: &str = ".trae-cn/trae-jwt-token";
const INTL_AGENT_TOKEN: &str = ".trae/trae-jwt-token";

/// IDE 登录态快照文件集（相对数据根）。整文件不透明拷贝，缺失项采集时跳过。
///
/// 实测体积合计约 2.7MB/账号：state.vscdb ~768K、Cookies 系列 ~20K、
/// Local Storage ~1.8M、Session Storage ~104K、Network Persistent State ~4K。
pub const IDE_SNAPSHOT_FILES: [&str; 4] = [
    "User/globalStorage/state.vscdb",
    "Cookies",
    "Cookies-journal",
    "Network Persistent State",
];
pub const IDE_SNAPSHOT_DIRS: [&str; 2] = ["Local Storage", "Session Storage"];

/// Trae 客户端档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    /// Trae IDE 国内版。
    IdeCn,
    /// Trae SOLO 国内版。
    SoloCn,
    /// Trae IDE 国际版。
    IdeIntl,
}

impl ClientKind {
    /// 全部档位（遍历入口，状态页按档位轮询）。
    pub const ALL: [ClientKind; 3] = [ClientKind::IdeCn, ClientKind::SoloCn, ClientKind::IdeIntl];

    /// 档位名（账号记录 `client` 字段、HTTP query/body 取值）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IdeCn => "ide_cn",
            Self::SoloCn => "solo_cn",
            Self::IdeIntl => "ide_intl",
        }
    }

    /// 解析档位字符串；未知/空返回 None（调用方报错，不静默回落）。
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        match raw.map(str::trim).unwrap_or_default() {
            "ide_cn" => Some(Self::IdeCn),
            "solo_cn" => Some(Self::SoloCn),
            "ide_intl" => Some(Self::IdeIntl),
            _ => None,
        }
    }

    /// 展示名。产品已统一为 Trae，展示名不再区分档位。
    pub fn display_name(self) -> &'static str {
        match self {
            Self::IdeCn => "Trae",
            Self::SoloCn => "Trae",
            Self::IdeIntl => "Trae",
        }
    }

    // -----------------------------------------------------------------------
    // 本地路径
    // -----------------------------------------------------------------------

    /// 客户端数据根（IDE 登录态所在目录）。
    pub fn data_root(self) -> PathBuf {
        home_dir().join(match self {
            Self::IdeCn => IDE_CN_DATA_ROOT,
            Self::SoloCn => SOLO_CN_DATA_ROOT,
            Self::IdeIntl => IDE_INTL_DATA_ROOT,
        })
    }

    /// agent（命令行 / Agent 模式）token 文件路径。
    pub fn agent_token_path(self) -> PathBuf {
        home_dir().join(match self {
            Self::IdeCn | Self::SoloCn => CN_AGENT_TOKEN,
            Self::IdeIntl => INTL_AGENT_TOKEN,
        })
    }

    // -----------------------------------------------------------------------
    // 应用身份（macOS）
    // -----------------------------------------------------------------------

    /// macOS app 名（候选目录与进程模式的主名）。
    pub fn macos_app_names(self) -> &'static [&'static str] {
        match self {
            Self::IdeCn => &IDE_CN_MACOS_APPS,
            Self::SoloCn => &SOLO_CN_MACOS_APPS,
            Self::IdeIntl => &IDE_INTL_MACOS_APPS,
        }
    }

    /// macOS 优雅退出用的 bundle id。
    pub fn macos_bundle_id(self) -> &'static str {
        match self {
            Self::IdeCn => IDE_CN_MACOS_BUNDLE_ID,
            Self::SoloCn => SOLO_CN_MACOS_BUNDLE_ID,
            Self::IdeIntl => IDE_INTL_MACOS_BUNDLE_ID,
        }
    }

    /// macOS app 路径探测全失败时的回落路径。
    pub fn macos_default_app_path(self) -> PathBuf {
        std::path::Path::new("/Applications").join(self.macos_app_names()[0])
    }

    /// app 路径缓存的分键（与档位名同形，独立方法便于将来单独演进盘上格式）。
    pub fn app_cache_key(self) -> &'static str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for kind in ClientKind::ALL {
            assert_eq!(ClientKind::parse(Some(kind.as_str())), Some(kind));
        }
        assert_eq!(ClientKind::parse(None), None);
        assert_eq!(ClientKind::parse(Some("cn")), None);
        assert_eq!(ClientKind::parse(Some("")), None);
    }

    #[test]
    fn data_roots_are_distinct() {
        let roots: Vec<PathBuf> = ClientKind::ALL.iter().map(|k| k.data_root()).collect();
        for (i, a) in roots.iter().enumerate() {
            for b in roots.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn cn_variants_share_agent_token() {
        assert_eq!(
            ClientKind::IdeCn.agent_token_path(),
            ClientKind::SoloCn.agent_token_path()
        );
        assert_ne!(
            ClientKind::IdeCn.agent_token_path(),
            ClientKind::IdeIntl.agent_token_path()
        );
    }
}
