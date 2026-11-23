# Stable Debug signing key

This Base64-encoded keystore is a fixed key for the Football Match Lab **Debug** variant only.

- Gradle decodes it into the ignored `.gradle/stable-debug.keystore` file during configuration.
- The Debug application ID is `pl.mkrstudio.truefootball3.debug`, separate from the production ID.
- Its password is the standard public Android debug password `android`, with alias `androiddebugkey`.
- The key is intentionally committed so CI can sign repeatable Debug APKs without GitHub Secrets.
- This key is public by design. Never use it for the production Release variant, Play Store, or any app identity that requires a private signer.
- Expected SHA-256 certificate fingerprint: `d3ee2af5ec69b6b29fff105118bc66200070681fa9ee6124a5972761d7db45f1`.

