/// Vector type, as named in the rawfile `Variables:` table.
///
/// The canonical names come from the `types[]` table in
/// `corpus/spice3f5/src/lib/fte/types.c`. The type is stored *by name*,
/// not by the `SV_*` index: the 3f5 table and the `SV_*` constants in
/// `fteconst.h` disagree from index 5 on (the table kept four noise
/// entries the constants collapsed into two), so round-tripping the name
/// is the only unambiguous representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarType {
    NoType,
    Time,
    Frequency,
    Voltage,
    Current,
    OnoiseSpectrum,
    OnoiseIntegrated,
    InoiseSpectrum,
    InoiseIntegrated,
    OutputNoise,
    InputNoise,
    Pole,
    Zero,
    SParam,
    /// A name not in the 3f5 table (e.g. ngspice additions). Kept
    /// verbatim so writing it back is lossless.
    Other(String),
}

impl VarType {
    /// The name written in the `Variables:` table (`ft_typenames`).
    pub fn name(&self) -> &str {
        match self {
            VarType::NoType => "notype",
            VarType::Time => "time",
            VarType::Frequency => "frequency",
            VarType::Voltage => "voltage",
            VarType::Current => "current",
            VarType::OnoiseSpectrum => "onoise-spectrum",
            VarType::OnoiseIntegrated => "onoise-integrated",
            VarType::InoiseSpectrum => "inoise-spectrum",
            VarType::InoiseIntegrated => "inoise-integrated",
            VarType::OutputNoise => "output-noise",
            VarType::InputNoise => "input-noise",
            VarType::Pole => "pole",
            VarType::Zero => "zero",
            VarType::SParam => "s-param",
            VarType::Other(s) => s,
        }
    }

    /// Case-insensitive lookup (`ft_typnum`); unknown names are kept.
    pub fn from_name(name: &str) -> Self {
        const TABLE: [(&str, VarType); 14] = [
            ("notype", VarType::NoType),
            ("time", VarType::Time),
            ("frequency", VarType::Frequency),
            ("voltage", VarType::Voltage),
            ("current", VarType::Current),
            ("onoise-spectrum", VarType::OnoiseSpectrum),
            ("onoise-integrated", VarType::OnoiseIntegrated),
            ("inoise-spectrum", VarType::InoiseSpectrum),
            ("inoise-integrated", VarType::InoiseIntegrated),
            ("output-noise", VarType::OutputNoise),
            ("input-noise", VarType::InputNoise),
            ("pole", VarType::Pole),
            ("zero", VarType::Zero),
            ("s-param", VarType::SParam),
        ];
        for (n, t) in TABLE {
            if n.eq_ignore_ascii_case(name) {
                return t;
            }
        }
        VarType::Other(name.to_owned())
    }

    /// Unit abbreviation (`ft_typabbrev`), used by the reader to rename
    /// vectors whose name starts with a digit: `2` of type voltage
    /// becomes `V(2)`.
    pub fn abbrev(&self) -> Option<&'static str> {
        match self {
            VarType::Time => Some("S"),
            VarType::Frequency => Some("Hz"),
            VarType::Voltage => Some("V"),
            VarType::Current => Some("A"),
            VarType::OnoiseSpectrum | VarType::InoiseSpectrum => Some("(V or A)^2/Hz"),
            VarType::OnoiseIntegrated | VarType::InoiseIntegrated => Some("V or A"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_names() {
        for name in [
            "notype",
            "time",
            "frequency",
            "voltage",
            "current",
            "onoise-spectrum",
            "s-param",
        ] {
            assert_eq!(VarType::from_name(name).name(), name);
        }
    }

    #[test]
    fn case_insensitive_and_unknown() {
        assert_eq!(VarType::from_name("VOLTAGE"), VarType::Voltage);
        assert_eq!(
            VarType::from_name("admittance"),
            VarType::Other("admittance".into())
        );
        assert_eq!(VarType::from_name("admittance").name(), "admittance");
    }
}
