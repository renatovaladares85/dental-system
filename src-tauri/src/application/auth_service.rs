use std::sync::{Arc, LazyLock};

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{
    AppError, AppResult, AuthSession, LoginInput, LoginUserRecord, NewSessionRecord, SessionRecord,
};

const IDLE_MINUTES: i64 = 30;
const ABSOLUTE_HOURS: i64 = 12;

static DUMMY_PASSWORD_PHC: LazyLock<String> = LazyLock::new(|| {
    let salt = SaltString::encode_b64(&[0x5a; 16]).expect("fixed Argon2 salt is valid");
    argon2()
        .hash_password(b"invalid-login-password", &salt)
        .expect("fixed dummy password hash is valid")
        .to_string()
});

pub trait SessionStore: Send + Sync {
    fn find_login_user(&self, username: &str) -> AppResult<Option<LoginUserRecord>>;
    fn create_session(&self, session: &NewSessionRecord) -> AppResult<()>;
    fn find_active_session(
        &self,
        token_hash: &[u8; 32],
        now: &str,
    ) -> AppResult<Option<SessionRecord>>;
    fn touch_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
    ) -> AppResult<()>;
    fn rotate_session(
        &self,
        session_id: &str,
        old_token_hash: &[u8; 32],
        new_token_hash: &[u8; 32],
        new_csrf_hash: &[u8; 32],
        now: &str,
        idle_expires_at: &str,
    ) -> AppResult<()>;
    fn revoke_session(
        &self,
        session_id: &str,
        token_hash: &[u8; 32],
        user_id: &str,
        now: &str,
    ) -> AppResult<()>;
}

pub struct AuthenticationService {
    store: Arc<dyn SessionStore>,
}

pub struct IssuedSession {
    pub public: AuthSession,
    pub session_token: Zeroizing<String>,
    pub csrf_token: Zeroizing<String>,
}

pub struct SessionView {
    pub public: AuthSession,
    pub csrf_token: Zeroizing<String>,
}

pub struct SessionContext {
    pub record: SessionRecord,
    token_hash: [u8; 32],
    idle_expires_at: String,
}

impl AuthenticationService {
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        LazyLock::force(&DUMMY_PASSWORD_PHC);
        Self { store }
    }

    pub fn login(&self, mut input: LoginInput) -> AppResult<IssuedSession> {
        let mut raw_username = Zeroizing::new(std::mem::take(&mut input.username));
        let mut raw_password = Zeroizing::new(std::mem::take(&mut input.password));
        if raw_username.len() > 64 || raw_password.len() > 1024 {
            verify_dummy_password(raw_password.as_bytes());
            return Err(AppError::invalid_credentials());
        }
        let username = raw_username.trim().to_ascii_lowercase();
        let password = Zeroizing::new(raw_password.nfc().collect::<String>());
        raw_username.zeroize();
        raw_password.zeroize();
        if password.chars().count() > 128 {
            verify_dummy_password(password.as_bytes());
            return Err(AppError::invalid_credentials());
        }

        let user = self.store.find_login_user(&username)?;
        let password_matches = match user.as_ref() {
            Some(record) => verify_password(password.as_bytes(), &record.password_phc),
            None => {
                verify_dummy_password(password.as_bytes());
                false
            }
        };
        if !password_matches {
            return Err(AppError::invalid_credentials());
        }
        let user = user.ok_or_else(AppError::invalid_credentials)?;

        let now = Utc::now();
        let idle_expires_at = now + Duration::minutes(IDLE_MINUTES);
        let absolute_expires_at = now + Duration::hours(ABSOLUTE_HOURS);
        let session_token = random_token();
        let csrf_token = csrf_for_session(&session_token)?;
        let session = NewSessionRecord {
            id: Uuid::now_v7().to_string(),
            user_id: user.user.id.clone(),
            token_hash: hash_token(&session_token),
            csrf_hash: hash_token(&csrf_token),
            created_at: timestamp(now),
            idle_expires_at: timestamp(idle_expires_at),
            absolute_expires_at: timestamp(absolute_expires_at),
        };
        self.store.create_session(&session)?;

        Ok(IssuedSession {
            public: authenticated_response(
                user.user,
                session.idle_expires_at,
                session.absolute_expires_at,
            ),
            session_token,
            csrf_token,
        })
    }

    pub fn session(&self, session_token: &str) -> AppResult<SessionView> {
        let context = self.authenticate(session_token)?;
        let csrf_token = csrf_for_session(session_token)?;
        let expected = hash_token(&csrf_token);
        if !bool::from(
            expected
                .as_slice()
                .ct_eq(context.record.csrf_hash.as_slice()),
        ) {
            return Err(AppError::forbidden(
                "CSRF_SESSION_MISMATCH",
                "A sessão não pôde ser validada com segurança.",
            ));
        }
        self.store.touch_session(
            &context.record.id,
            &context.token_hash,
            &timestamp(Utc::now()),
            &context.idle_expires_at,
        )?;
        let public = authenticated_response(
            context.record.user,
            context.idle_expires_at,
            context.record.absolute_expires_at,
        );
        Ok(SessionView { public, csrf_token })
    }

    pub fn logout(&self, session_token: &str, csrf_token: &str) -> AppResult<()> {
        let context = self.authenticate_with_csrf(session_token, csrf_token)?;
        self.store.revoke_session(
            &context.record.id,
            &context.token_hash,
            &context.record.user.id,
            &timestamp(Utc::now()),
        )
    }

    pub fn rotate(&self, session_token: &str, csrf_token: &str) -> AppResult<IssuedSession> {
        let context = self.authenticate_with_csrf(session_token, csrf_token)?;
        let new_session_token = random_token();
        let new_csrf_token = csrf_for_session(&new_session_token)?;
        let now = Utc::now();
        let idle_expires_at = bounded_idle_expiry(now, &context.record.absolute_expires_at)?;
        self.store.rotate_session(
            &context.record.id,
            &context.token_hash,
            &hash_token(&new_session_token),
            &hash_token(&new_csrf_token),
            &timestamp(now),
            &idle_expires_at,
        )?;
        Ok(IssuedSession {
            public: authenticated_response(
                context.record.user,
                idle_expires_at,
                context.record.absolute_expires_at,
            ),
            session_token: new_session_token,
            csrf_token: new_csrf_token,
        })
    }

    fn authenticate(&self, session_token: &str) -> AppResult<SessionContext> {
        if session_token.len() != 43 || session_token.len() > 128 {
            return Err(AppError::unauthenticated());
        }
        let token_hash = hash_token(session_token);
        let now = Utc::now();
        let record = self
            .store
            .find_active_session(&token_hash, &timestamp(now))?
            .ok_or_else(AppError::unauthenticated)?;
        let idle_expires_at = bounded_idle_expiry(now, &record.absolute_expires_at)?;
        Ok(SessionContext {
            record,
            token_hash,
            idle_expires_at,
        })
    }

    fn authenticate_with_csrf(
        &self,
        session_token: &str,
        csrf_token: &str,
    ) -> AppResult<SessionContext> {
        let context = self.authenticate(session_token)?;
        let presented = hash_token(csrf_token);
        if !bool::from(
            presented
                .as_slice()
                .ct_eq(context.record.csrf_hash.as_slice()),
        ) {
            return Err(AppError::forbidden(
                "CSRF_VALIDATION_FAILED",
                "A validação de segurança da solicitação falhou.",
            ));
        }
        Ok(context)
    }
}

fn authenticated_response(
    user: crate::domain::AuthUser,
    idle_expires_at: String,
    absolute_expires_at: String,
) -> AuthSession {
    AuthSession {
        authenticated: true,
        user: Some(user),
        idle_expires_at: Some(idle_expires_at),
        absolute_expires_at: Some(absolute_expires_at),
    }
}

fn random_token() -> Zeroizing<String> {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(bytes.as_mut());
    Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref()))
}

fn csrf_for_session(session_token: &str) -> AppResult<Zeroizing<String>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(session_token.as_bytes())
        .map_err(|_| AppError::security())?;
    mac.update(b"offline-dental-csrf-v1");
    let mut bytes = Zeroizing::new([0_u8; 32]);
    bytes.copy_from_slice(&mac.finalize().into_bytes());
    Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref())))
}

fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn verify_password(password: &[u8], encoded: &str) -> bool {
    PasswordHash::new(encoded)
        .ok()
        .is_some_and(|hash| argon2().verify_password(password, &hash).is_ok())
}

fn verify_dummy_password(password: &[u8]) {
    let _ = verify_password(password, &DUMMY_PASSWORD_PHC);
}

fn argon2() -> Argon2<'static> {
    let parameters = Params::new(65_536, 3, 1, Some(32)).expect("valid Argon2 parameters");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters)
}

fn bounded_idle_expiry(now: DateTime<Utc>, absolute: &str) -> AppResult<String> {
    let absolute = DateTime::parse_from_rfc3339(absolute)
        .map_err(|_| AppError::database())?
        .with_timezone(&Utc);
    Ok(timestamp(std::cmp::min(
        now + Duration::minutes(IDLE_MINUTES),
        absolute,
    )))
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AuthUser;
    use std::sync::Mutex;

    #[test]
    fn opaque_tokens_have_256_bits_and_no_padding() {
        let token = random_token();
        assert_eq!(token.len(), 43);
        assert!(!token.contains('='));
        assert_ne!(token.as_str(), random_token().as_str());
    }

    #[test]
    fn dummy_hash_uses_required_argon2_parameters() {
        let parsed = PasswordHash::new(&DUMMY_PASSWORD_PHC).expect("parse dummy hash");
        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(parsed.version, Some(19));
        assert!(verify_password(
            b"invalid-login-password",
            &DUMMY_PASSWORD_PHC
        ));
    }

    #[derive(Default)]
    struct FakeStore {
        state: Mutex<FakeState>,
    }

    #[derive(Default)]
    struct FakeState {
        login_user: Option<LoginUserRecord>,
        session: Option<FakeSession>,
        touches: usize,
    }

    struct FakeSession {
        record: NewSessionRecord,
        user: AuthUser,
        revoked: bool,
    }

    impl FakeStore {
        fn with_master() -> Self {
            Self::with_password_hash(DUMMY_PASSWORD_PHC.clone())
        }

        fn with_password_hash(password_phc: String) -> Self {
            Self {
                state: Mutex::new(FakeState {
                    login_user: Some(LoginUserRecord {
                        user: AuthUser {
                            id: "user-1".to_owned(),
                            full_name: "Administrador Mestre".to_owned(),
                            username: "admin.master".to_owned(),
                            email: "admin@example.test".to_owned(),
                            roles: vec!["MASTER_ADMIN".to_owned()],
                        },
                        password_phc: Zeroizing::new(password_phc),
                    }),
                    ..FakeState::default()
                }),
            }
        }
    }

    impl SessionStore for FakeStore {
        fn find_login_user(&self, username: &str) -> AppResult<Option<LoginUserRecord>> {
            let state = self.state.lock().map_err(|_| AppError::worker())?;
            Ok(state
                .login_user
                .as_ref()
                .filter(|record| record.user.username == username)
                .cloned())
        }

        fn create_session(&self, session: &NewSessionRecord) -> AppResult<()> {
            let mut state = self.state.lock().map_err(|_| AppError::worker())?;
            let user = state
                .login_user
                .as_ref()
                .map(|record| record.user.clone())
                .ok_or_else(AppError::database)?;
            state.session = Some(FakeSession {
                record: session.clone(),
                user,
                revoked: false,
            });
            Ok(())
        }

        fn find_active_session(
            &self,
            token_hash: &[u8; 32],
            now: &str,
        ) -> AppResult<Option<SessionRecord>> {
            let state = self.state.lock().map_err(|_| AppError::worker())?;
            Ok(state.session.as_ref().and_then(|session| {
                (!session.revoked
                    && session.record.token_hash == *token_hash
                    && session.record.idle_expires_at.as_str() > now
                    && session.record.absolute_expires_at.as_str() > now)
                    .then(|| SessionRecord {
                        id: session.record.id.clone(),
                        user: session.user.clone(),
                        csrf_hash: session.record.csrf_hash,
                        idle_expires_at: session.record.idle_expires_at.clone(),
                        absolute_expires_at: session.record.absolute_expires_at.clone(),
                    })
            }))
        }

        fn touch_session(
            &self,
            session_id: &str,
            token_hash: &[u8; 32],
            _now: &str,
            idle_expires_at: &str,
        ) -> AppResult<()> {
            let mut state = self.state.lock().map_err(|_| AppError::worker())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(AppError::unauthenticated)?;
            if session.record.id != session_id
                || session.record.token_hash != *token_hash
                || session.revoked
            {
                return Err(AppError::unauthenticated());
            }
            session.record.idle_expires_at = idle_expires_at.to_owned();
            state.touches += 1;
            Ok(())
        }

        fn rotate_session(
            &self,
            session_id: &str,
            old_token_hash: &[u8; 32],
            new_token_hash: &[u8; 32],
            new_csrf_hash: &[u8; 32],
            _now: &str,
            idle_expires_at: &str,
        ) -> AppResult<()> {
            let mut state = self.state.lock().map_err(|_| AppError::worker())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(AppError::unauthenticated)?;
            if session.record.id != session_id
                || session.record.token_hash != *old_token_hash
                || session.revoked
            {
                return Err(AppError::unauthenticated());
            }
            session.record.token_hash = *new_token_hash;
            session.record.csrf_hash = *new_csrf_hash;
            session.record.idle_expires_at = idle_expires_at.to_owned();
            Ok(())
        }

        fn revoke_session(
            &self,
            session_id: &str,
            token_hash: &[u8; 32],
            _user_id: &str,
            _now: &str,
        ) -> AppResult<()> {
            let mut state = self.state.lock().map_err(|_| AppError::worker())?;
            let session = state
                .session
                .as_mut()
                .ok_or_else(AppError::unauthenticated)?;
            if session.record.id != session_id || session.record.token_hash != *token_hash {
                return Err(AppError::unauthenticated());
            }
            session.revoked = true;
            Ok(())
        }
    }

    fn login(service: &AuthenticationService) -> IssuedSession {
        service
            .login(LoginInput {
                username: "admin.master".to_owned(),
                password: "invalid-login-password".to_owned(),
            })
            .expect("login")
    }

    #[test]
    fn login_session_rotation_and_logout_are_fail_closed() {
        let store = Arc::new(FakeStore::with_master());
        let service = AuthenticationService::new(store.clone());
        let issued = login(&service);
        assert!(issued.public.authenticated);
        assert_eq!(issued.session_token.len(), 43);
        let serialized = serde_json::to_string(&issued.public).expect("serialize response");
        assert!(!serialized.contains("csrf"));
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains(issued.session_token.as_str()));

        let view = service
            .session(&issued.session_token)
            .expect("session bootstrap");
        assert!(view.public.authenticated);
        assert_eq!(view.csrf_token.as_str(), issued.csrf_token.as_str());
        let second_view = service
            .session(&issued.session_token)
            .expect("second tab session bootstrap");
        assert_eq!(second_view.csrf_token.as_str(), view.csrf_token.as_str());

        let touches_before = store.state.lock().expect("state").touches;
        let invalid = service.logout(&issued.session_token, &"A".repeat(43));
        assert_eq!(
            invalid.expect_err("invalid csrf").code,
            "CSRF_VALIDATION_FAILED"
        );
        assert_eq!(store.state.lock().expect("state").touches, touches_before);

        let rotated = service
            .rotate(&issued.session_token, &view.csrf_token)
            .expect("rotate");
        assert_ne!(
            rotated.session_token.as_str(),
            issued.session_token.as_str()
        );
        assert!(service.session(&issued.session_token).is_err());
        service
            .logout(&rotated.session_token, &rotated.csrf_token)
            .expect("logout");
        assert!(service.session(&rotated.session_token).is_err());
    }

    #[test]
    fn expired_session_is_rejected_without_touching_it() {
        let store = Arc::new(FakeStore::with_master());
        let service = AuthenticationService::new(store.clone());
        let issued = login(&service);
        {
            let mut state = store.state.lock().expect("state");
            state
                .session
                .as_mut()
                .expect("session")
                .record
                .idle_expires_at = "2000-01-01T00:00:00.000Z".to_owned();
        }
        let error = match service.session(&issued.session_token) {
            Ok(_) => panic!("expired session was accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "AUTHENTICATION_REQUIRED");
        assert_eq!(store.state.lock().expect("state").touches, 0);
    }

    #[test]
    fn login_normalizes_canonically_equivalent_unicode_passwords_to_nfc() {
        let nfc_password = "frase longa e exclusiva Café 2026";
        let nfd_password = "frase longa e exclusiva Cafe\u{301} 2026";
        assert_ne!(nfc_password.as_bytes(), nfd_password.as_bytes());
        let salt = SaltString::encode_b64(&[0x33; 16]).expect("fixed salt");
        let password_phc = argon2()
            .hash_password(nfc_password.as_bytes(), &salt)
            .expect("NFC password hash")
            .to_string();
        let store = Arc::new(FakeStore::with_password_hash(password_phc));
        let service = AuthenticationService::new(store);

        let issued = service
            .login(LoginInput {
                username: "admin.master".to_owned(),
                password: nfd_password.to_owned(),
            })
            .expect("canonically equivalent login");
        assert!(issued.public.authenticated);
    }
}
