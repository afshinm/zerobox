use zerobox_utils_absolute_path::AbsolutePathBuf;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct FileSystemPermissions {
    pub read: Option<Vec<AbsolutePathBuf>>,
    pub write: Option<Vec<AbsolutePathBuf>>,
}

impl FileSystemPermissions {
    pub fn is_empty(&self) -> bool {
        self.read.is_none() && self.write.is_none()
    }
}

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct NetworkPermissions {
    pub enabled: Option<bool>,
}

impl NetworkPermissions {
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
    }
}

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct PermissionProfile {
    pub network: Option<NetworkPermissions>,
    pub file_system: Option<FileSystemPermissions>,
}

impl PermissionProfile {
    pub fn is_empty(&self) -> bool {
        self.network.is_none() && self.file_system.is_none()
    }
}
