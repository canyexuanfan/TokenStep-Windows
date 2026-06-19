# Code signing (Windows)

The released `TokenStep_0.1.0_x64-setup.exe` and `tokenstep.exe` are signed
with a **self-signed code-signing certificate** (CN = `十七°（基于 TokenStep）`).

## What self-signing does (and doesn't)

| | Self-signed (current) | Paid EV cert (future) |
|---|---|---|
| Has a digital signature | ✅ | ✅ |
| Shows publisher name in UAC | ✅ 十七° | ✅ |
| Antivirus false-positives | slightly fewer | clearly fewer |
| **SmartScreen blue window** | ❌ still shows (no reputation) | ✅ gone immediately |

Windows SmartScreen judges by **reputation** (how many users have safely run
files signed by this certificate), not by the signature itself. So a free
self-signed cert still triggers the "Windows protected your PC" prompt the
first time — only a paid **EV** certificate removes it instantly.

On **this machine** (where the cert was imported into the trusted root store),
the signature verifies cleanly and no warning is shown.

## The certificate

- Subject: `CN=十七°（基于 TokenStep）, O=TokenStep Windows Port, C=CN`
- Algorithm: RSA 2048, SHA256
- Validity: 3 years from creation
- Stored in `Cert:\CurrentUser\My` (private key present)
- Also imported into `Cert:\CurrentUser\Root` so the local machine trusts it
- Private key backed up at `_secrets/TokenStep-codesign.pfx`
  (password-protected; **never commit** — it's in `.gitignore`)

## Sign the build

After `cargo tauri build`:

```bat
windows\scripts\sign.bat
```

Or with an explicit thumbprint:

```bat
windows\scripts\sign.bat A9E2372BC217D83C27717553132091130C953074
```

Verify:

```bat
signtool verify /pa /all TokenStep_0.1.0_x64-setup.exe
powershell "Get-AuthenticodeSignature TokenStep_0.1.0_x64-setup.exe | Format-List Status,SignerCertificate"
```

## (Re)create the certificate

If you need to regenerate it (new machine, cert expired, etc.):

```powershell
# 1. Create the self-signed code-signing cert.
$pwd = ConvertTo-SecureString -String 'YOUR_PASSWORD' -Force -AsPlainText
$cert = New-SelfSignedCertificate `
  -Type CodeSigningCert `
  -Subject 'CN=十七°（基于 TokenStep）, O=TokenStep Windows Port, C=CN' `
  -FriendlyName 'TokenStep Windows (十七°)' `
  -KeyUsage DigitalSignature -KeyAlgorithm RSA -KeyLength 2048 `
  -HashAlgorithm SHA256 `
  -NotAfter (Get-Date).AddYears(3) `
  -CertStoreLocation 'Cert:\CurrentUser\My' `
  -KeyExportPolicy Exportable

# 2. Trust it on this machine (CurrentUser root — no admin needed).
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root','CurrentUser')
$store.Open('ReadWrite'); $store.Add($cert); $store.Close()

# 3. Back up the PFX (private key) somewhere safe.
Export-PfxCertificate -Cert $cert -FilePath '_secrets\TokenStep-codesign.pfx' -Password $pwd
```

> Trusting via `CurrentUser\Root` works for your own user account. For
> machine-wide trust (all users), import into `LocalMachine\Root` (needs admin).

## Distributing to other users

Because this is a **self-signed** cert, recipients' machines won't trust it by
default — they'll see "Unknown publisher" unless they also install
`TokenStep-codesign.cer` into their trusted root. Options:

1. **Just accept the warning** — fine for personal / small-circle use.
2. **Tell recipients to install the .cer** — import `TokenStep-codesign.cer`
   into `Cert:\CurrentUser\Root` (double-click → Install Certificate → Local
   Machine → Place in: Trusted Root Certification Authorities).
3. **Switch to Sigstore signing** (planned, see ROADMAP) — free, verifiable
   provenance via GitHub Actions OIDC, but SmartScreen still prompts.
4. **Buy an EV cert** — the only way to remove SmartScreen instantly.
