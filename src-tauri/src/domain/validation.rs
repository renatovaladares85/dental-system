use std::path::PathBuf;

use subtle::ConstantTimeEq;
use unicode_normalization::UnicodeNormalization;
use zeroize::{Zeroize, Zeroizing};

use super::{AppError, AppResult, FieldError, InitialSetupInput};

const COMMON_PASSWORDS_V1: &str = include_str!("../../resources/common-passwords-v1.txt");

pub struct ValidatedInitialSetup {
    pub organization_name: String,
    pub unit_name: String,
    pub responsible_name: String,
    pub phone: Option<String>,
    pub administrative_email: Option<String>,
    pub address: Option<String>,
    pub professional_registration: Option<String>,
    pub master_full_name: String,
    pub master_username: String,
    pub master_email: String,
    pub master_password: Zeroizing<String>,
    pub backup_directory: PathBuf,
    pub recovery_directory: PathBuf,
}

pub fn validate_initial_setup(mut input: InitialSetupInput) -> AppResult<ValidatedInitialSetup> {
    let mut errors = Vec::new();

    let organization_name = bounded_required(
        "organization.name",
        &input.organization.name,
        2,
        120,
        &mut errors,
    );
    let unit_name = bounded_required("unit.name", &input.unit.name, 2, 120, &mut errors);
    let responsible_name = bounded_required(
        "unit.responsibleName",
        &input.unit.responsible_name,
        2,
        120,
        &mut errors,
    );
    let master_full_name = bounded_required(
        "master.fullName",
        &input.master.full_name,
        2,
        120,
        &mut errors,
    );

    let username = input.master.username.trim().to_owned();
    if !valid_username(&username) {
        errors.push(field_error(
            "master.username",
            "Use de 3 a 64 caracteres: letras minúsculas, números, ponto, hífen ou sublinhado.",
        ));
    }

    let master_email =
        normalize_email("master.email", Some(&input.master.email), true, &mut errors)
            .unwrap_or_default();
    let administrative_email = normalize_email(
        "unit.administrativeEmail",
        input.unit.administrative_email.as_deref(),
        false,
        &mut errors,
    );
    validate_optional_length("unit.phone", input.unit.phone.as_deref(), 40, &mut errors);
    validate_optional_length(
        "unit.address",
        input.unit.address.as_deref(),
        500,
        &mut errors,
    );
    validate_optional_length(
        "unit.professionalRegistration",
        input.unit.professional_registration.as_deref(),
        80,
        &mut errors,
    );

    let normalized_password = validate_password(
        &input.master.password,
        &input.master.password_confirmation,
        &username,
        &master_full_name,
        &organization_name,
        &mut errors,
    );

    let backup_directory = PathBuf::from(input.storage.backup_directory.trim());
    if input.storage.backup_directory.trim().is_empty() {
        errors.push(field_error(
            "storage.backupDirectory",
            "Selecione o diretório do backup inicial.",
        ));
    }
    let recovery_directory = PathBuf::from(input.storage.recovery_package_directory.trim());
    if input.storage.recovery_package_directory.trim().is_empty() {
        errors.push(field_error(
            "storage.recoveryPackageDirectory",
            "Selecione o diretório do pacote de recuperação.",
        ));
    }

    if !errors.is_empty() {
        input.master.password.zeroize();
        input.master.password_confirmation.zeroize();
        return Err(AppError::validation(errors));
    }

    input.master.password.zeroize();
    let password = normalized_password;
    input.master.password_confirmation.zeroize();

    Ok(ValidatedInitialSetup {
        organization_name,
        unit_name,
        responsible_name,
        phone: optional_trimmed(input.unit.phone),
        administrative_email,
        address: optional_trimmed(input.unit.address),
        professional_registration: optional_trimmed(input.unit.professional_registration),
        master_full_name,
        master_username: username,
        master_email,
        master_password: password,
        backup_directory,
        recovery_directory,
    })
}

fn bounded_required(
    field: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
    errors: &mut Vec<FieldError>,
) -> String {
    let normalized = value.trim().to_owned();
    let count = normalized.chars().count();
    if !(minimum..=maximum).contains(&count) {
        errors.push(field_error(
            field,
            &format!("Informe entre {minimum} e {maximum} caracteres."),
        ));
    }
    normalized
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let normalized = candidate.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn validate_optional_length(
    field: &str,
    value: Option<&str>,
    maximum: usize,
    errors: &mut Vec<FieldError>,
) {
    if value
        .map(str::trim)
        .is_some_and(|candidate| candidate.chars().count() > maximum)
    {
        errors.push(field_error(
            field,
            &format!("Use no máximo {maximum} caracteres."),
        ));
    }
}

fn normalize_email(
    field: &str,
    value: Option<&str>,
    required: bool,
    errors: &mut Vec<FieldError>,
) -> Option<String> {
    let raw = value.unwrap_or_default();
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        if required {
            errors.push(field_error(field, "Informe um e-mail válido."));
        }
        return None;
    }

    let valid = !raw
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
        && valid_email_syntax(&normalized);
    if !valid {
        errors.push(field_error(field, "Informe um e-mail válido."));
    }
    Some(normalized)
}

fn valid_email_syntax(email: &str) -> bool {
    if !email.is_ascii() || email.len() > 254 {
        return false;
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&byte))
    {
        return false;
    }

    let labels = domain.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && domain.len() <= 253
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn valid_username(username: &str) -> bool {
    let bytes = username.as_bytes();
    (3..=64).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(byte))
}

fn validate_password(
    password: &str,
    confirmation: &str,
    username: &str,
    full_name: &str,
    organization_name: &str,
    errors: &mut Vec<FieldError>,
) -> Zeroizing<String> {
    if password.len() > 1024 || confirmation.len() > 1024 {
        errors.push(field_error(
            "master.password",
            "Use uma senha com no máximo 128 caracteres.",
        ));
        return Zeroizing::new(String::new());
    }
    let normalized = Zeroizing::new(password.nfc().collect::<String>());
    let normalized_confirmation = Zeroizing::new(confirmation.nfc().collect::<String>());
    let count = normalized.chars().count();
    if !(15..=128).contains(&count) || normalized.len() > 1024 {
        errors.push(field_error(
            "master.password",
            "Use uma senha com 15 a 128 caracteres.",
        ));
    }

    let password_comparison = Zeroizing::new(normalize_for_comparison(&normalized));
    let username_comparison = normalize_for_comparison(username);
    let full_name_comparison = normalize_for_comparison(full_name);
    let organization_comparison = normalize_for_comparison(organization_name);
    let contextual_terms = full_name_comparison
        .split_whitespace()
        .chain(organization_comparison.split_whitespace())
        .filter(|term| term.chars().count() >= 3)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if is_common_password(&password_comparison)
        || (!username_comparison.is_empty() && password_comparison.contains(&username_comparison))
        || contextual_terms
            .iter()
            .any(|term| password_comparison.contains(term))
    {
        errors.push(field_error(
            "master.password",
            "Escolha uma senha que não contenha termos previsíveis.",
        ));
    }

    let equal = normalized.len() == normalized_confirmation.len()
        && bool::from(
            normalized
                .as_bytes()
                .ct_eq(normalized_confirmation.as_bytes()),
        );
    if !equal {
        errors.push(field_error(
            "master.passwordConfirmation",
            "A confirmação não corresponde à senha.",
        ));
    }
    normalized
}

fn normalize_for_comparison(value: &str) -> String {
    value
        .nfc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn is_common_password(value: &str) -> bool {
    COMMON_PASSWORDS_V1
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .any(|candidate| candidate == value)
}

fn field_error(field: &str, message: &str) -> FieldError {
    FieldError {
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        InitialSetupInput, MasterUserInput, OrganizationInput, StorageInput, UnitInput,
    };

    fn valid_input() -> InitialSetupInput {
        InitialSetupInput {
            organization: OrganizationInput {
                name: "Clínica Horizonte".to_owned(),
            },
            unit: UnitInput {
                name: "Unidade principal".to_owned(),
                responsible_name: "Ana Souza".to_owned(),
                phone: None,
                administrative_email: None,
                address: None,
                professional_registration: None,
            },
            master: MasterUserInput {
                full_name: "Carlos Oliveira".to_owned(),
                username: "carlos.admin".to_owned(),
                email: "carlos@example.test".to_owned(),
                password: "frase longa e exclusiva 2026".to_owned(),
                password_confirmation: "frase longa e exclusiva 2026".to_owned(),
            },
            storage: StorageInput {
                backup_directory: "/tmp/backup".to_owned(),
                recovery_package_directory: "/tmp/recovery".to_owned(),
            },
        }
    }

    #[test]
    fn normalizes_valid_input() {
        let validated = validate_initial_setup(valid_input()).expect("valid input");
        assert_eq!(validated.master_username, "carlos.admin");
        assert_eq!(validated.master_email, "carlos@example.test");
    }

    #[test]
    fn rejects_mismatched_confirmation() {
        let mut input = valid_input();
        input.master.password_confirmation = "outra frase longa e exclusiva".to_owned();
        let error = match validate_initial_setup(input) {
            Ok(_) => panic!("confirmation should be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.code, "VALIDATION_FAILED");
    }

    #[test]
    fn normalizes_password_to_nfc() {
        let mut input = valid_input();
        input.master.password = "segredo muito longo cafe\u{301} 2026".to_owned();
        input.master.password_confirmation = "segredo muito longo café 2026".to_owned();
        let validated = validate_initial_setup(input).expect("canonical equivalents");
        assert_eq!(
            validated.master_password.as_str(),
            "segredo muito longo café 2026"
        );
    }

    #[test]
    fn rejects_uppercase_username_instead_of_normalizing_it() {
        let mut input = valid_input();
        input.master.username = "Ana.Admin".to_owned();
        let error = expect_validation_error(input);
        assert!(
            error
                .field_errors
                .iter()
                .any(|field| field.field == "master.username")
        );
    }

    #[test]
    fn rejects_email_with_whitespace() {
        let mut input = valid_input();
        input.master.email = "a b@c.d".to_owned();
        let error = expect_validation_error(input);
        assert!(
            error
                .field_errors
                .iter()
                .any(|field| field.field == "master.email")
        );
    }

    #[test]
    fn rejects_short_username_when_embedded_in_password() {
        let mut input = valid_input();
        input.master.username = "ana".to_owned();
        input.master.password = "frase exclusiva ana 2026".to_owned();
        input.master.password_confirmation = input.master.password.clone();
        let error = expect_validation_error(input);
        assert!(
            error
                .field_errors
                .iter()
                .any(|field| field.field == "master.password")
        );
    }

    fn expect_validation_error(input: InitialSetupInput) -> AppError {
        match validate_initial_setup(input) {
            Ok(_) => panic!("validation should fail"),
            Err(error) => error,
        }
    }
}
