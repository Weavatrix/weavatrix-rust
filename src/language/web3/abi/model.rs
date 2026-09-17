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
