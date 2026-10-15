# Signing the Windows installer

The Windows installer is signed through [SignPath Foundation](https://signpath.org/), which gives
open-source projects a code-signing certificate for free. Until it is set up, the release workflow
builds the installer unsigned and Windows SmartScreen warns about it.

## One-time setup

1. Apply at <https://signpath.org/foundation> for this repository (it is public and AGPL-3.0, as
   the Foundation requires). Their review takes a while; they also ask for the project's pages
   to say who signs the releases and link the privacy policy.
2. In SignPath create a project with the slug `akidev`, add the artifact configuration in
   [`signpath-artifact.xml`](signpath-artifact.xml), and a signing policy with the slug
   `release-signing`. Connect the project to this GitHub repository (the "Trusted Build System"
   of GitHub.com) so SignPath checks that the file it signs came from this workflow.
3. In the repository settings add the secret `SIGNPATH_API_TOKEN` (a SignPath API token of a user
   allowed to submit to that policy) and the variable `SIGNPATH_ORGANIZATION_ID`.

After that, a push of a `desktop-windows-v<version>` tag builds, signs the installer and keeps it
as the `akidev-windows` artifact. The sign step is skipped while the secret does not exist.

The zip is not signed (SignPath's configuration above covers the installer); the installer is what
to hand to people.
