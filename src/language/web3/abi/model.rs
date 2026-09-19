use weavatrix_graph::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemberKind {
    Function,
    Event,
    Error,
    Constructor,
    Fallback,
    Receive,
}

impl MemberKind {
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Event => "event",
            Self::Error => "error",
            Self::Constructor => "constructor",
            Self::Fallback => "fallback",
            Self::Receive => "receive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AbiType {
    pub canonical: String,
    pub name: String,
    pub indexed: bool,
    pub components: Vec<AbiType>,
    pub internal_type: Option<String>,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AbiMember {
    pub kind: MemberKind,
    pub name: String,
    pub inputs: Vec<AbiType>,
    pub outputs: Vec<AbiType>,
    pub state_mutability: Option<String>,
    pub anonymous: bool,
    pub call_signature: String,
    pub return_shape: String,
    pub event_layout: String,
    pub client_surface: String,
    pub selector: Option<String>,
    pub span: SourceSpan,
    pub unsupported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    AbiJson,
    SolcInput,
    SolcOutput,
    FoundryArtifact,
    FoundryBuildInfo,
    Hardhat3Incomplete,
}

impl Profile {
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AbiJson => "abi-json",
            Self::SolcInput => "solc-input",
            Self::SolcOutput => "solc-output",
            Self::FoundryArtifact => "foundry-artifact",
            Self::FoundryBuildInfo => "foundry-build-info",
            Self::Hardhat3Incomplete => "hardhat3-incomplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Completeness {
    Full,
    Partial,
    ArtifactOnly,
}

impl Completeness {
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::ArtifactOnly => "artifact_only",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AbiDocument {
    pub profile: Profile,
    pub contract_name: Option<String>,
    pub members: Vec<AbiMember>,
    pub provenance: String,
    pub completeness: Completeness,
}

#[cfg(test)]
mod tests {
    use super::{Completeness, MemberKind, Profile};

    #[test]
    fn abi_enums_have_stable_names() {
        assert_eq!(MemberKind::Function.as_str(), "function");
        assert_eq!(MemberKind::Event.as_str(), "event");
        assert_eq!(MemberKind::Error.as_str(), "error");
        assert_eq!(MemberKind::Constructor.as_str(), "constructor");
        assert_eq!(MemberKind::Fallback.as_str(), "fallback");
        assert_eq!(MemberKind::Receive.as_str(), "receive");
        assert_eq!(Profile::AbiJson.as_str(), "abi-json");
        assert_eq!(Profile::SolcInput.as_str(), "solc-input");
        assert_eq!(Profile::SolcOutput.as_str(), "solc-output");
        assert_eq!(Profile::FoundryArtifact.as_str(), "foundry-artifact");
        assert_eq!(Profile::FoundryBuildInfo.as_str(), "foundry-build-info");
        assert_eq!(Profile::Hardhat3Incomplete.as_str(), "hardhat3-incomplete");
        assert_eq!(Completeness::Full.as_str(), "full");
        assert_eq!(Completeness::Partial.as_str(), "partial");
        assert_eq!(Completeness::ArtifactOnly.as_str(), "artifact_only");
    }
}
