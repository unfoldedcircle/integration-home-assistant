# Contributing

Thanks for taking the time to contribute!

Found a bug, a missing feature or a description that doesn't make sense or needs clarification? Please let us know.

### Bug Reports :bug:

If you find a bug, please search for it first in the [GitHub issues](https://github.com/unfoldedcircle/integration-home-assistant/issues),
and if it isn't already tracked, [create a new issue](https://github.com/unfoldedcircle/integration-home-assistant/issues/new).

⚠️ Please don't report security vulnerabilities as public issues: follow Unfolded Circle's
[security policy](https://github.com/unfoldedcircle/.github/blob/main/SECURITY.md) instead.

### Pull Requests

**Any pull request needs to be reviewed and approved by the Unfolded Circle development team.**

We love contributions from everyone.

⚠️ If you plan to make substantial changes, please reach out to us first.  
Either by opening a feature request describing your proposed changes before submitting code, or by contacting us on
one of the other [feedback channels](#feedback-speech_balloon).

Since this software (or part of it) is being used on the embedded Remote Two/3 devices, we have to make sure it remains
compatible with the embedded runtime environment and runs smoothly.

With that out of the way, here's the process of creating a pull request and making sure it passes the automated tests:

### Contributing Code :bulb:

1. Fork the repo.
   
2. Make your changes or enhancements (preferably on a feature-branch).

    Contributed code must be licensed under the Mozilla Public License 2.0 (MPL-2.0).  
    It is required to add a boilerplate copyright notice to the top of each file:

    ```
    // Copyright {year} {person OR org} <{email}>
    // SPDX-License-Identifier: MPL-2.0
    ```

3. Make sure your changes make the tests pass:
    ```shell
    cargo test
    ```

4. Make sure your changes make the lints pass:
    ```shell
    cargo clippy --features mdns-sd -- -D warnings
    ```

    - If clippy is missing, install it with: `rustup component add clippy`.
    - ℹ️ Keep clippy up to date (e.g. with `rustup update`), it's regularly improved with updates.

5. Add a `CHANGELOG.md` entry for every user-visible change (see below).

6. Make sure your changes follow the project's code style.  
   We are using the official [Rust style guide](https://doc.rust-lang.org/style-guide/).
    ```shell
    cargo fmt --all -- --check
    ```

7. If you added new Rust crate dependencies verify their licenses:
    ```shell
    cargo install cargo-about
    cargo about generate about.hbs > integration-hass_licenses.html
    ```

8. Push to your fork and open a pull request. Pull requests with failing checks are not reviewed until they pass.

At this point we will review the PR and give constructive feedback.  
This is a time for discussion and improvements, and making the necessary changes will be required before we can
merge the contribution.

### Commit Messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

<body: why the change was made>

<trailers>
```

- **Header:** the type (`feat`, `fix`, `docs`, `refactor`, `test`, `build`, `ci`, `chore`), an optional scope
  (for example `(ci)`) and a short description, at most 72 characters without the PR suffix.
- **Body:** why the change was made: the problem, its effect, and what the change achieves. The diff shows how.
  Wrap at 72 characters.
- **One logical change per commit.** A user-visible change carries its changelog entry in the same commit.
- Rebase on `main` instead of merging it into your branch.

### Changelog

Every user-visible fix or feature gets an entry in `CHANGELOG.md`, in the same commit. The file follows
[Keep a Changelog](https://keepachangelog.com/): entries go under `## Unreleased` in `### Added`, `### Changed`,
`### Removed` or `### Fixed`, written for the users of the library, not as commit messages.

### AI-assisted Contributions

The use of AI coding assistants is permitted, **but must be disclosed and used responsibly**:

- **Disclosure required**: if you used AI tools to generate or assist with any part of your contribution, say so in
  the pull request description and name the files or sections.
- **Attribution in commits**: a commit that an AI tool wrote or helped to write names the model in a
  `Co-authored-by:` trailer. Commit messages and pull requests must not contain agent session information, such as
  session IDs or session links.
- **You are responsible** for all code you submit: understand every line, test it, and be able to explain the design
  decisions in the review. Low-quality, unreviewed AI-generated contributions are rejected without review.

### Feedback :speech_balloon:

There are a few different ways to provide feedback:

- [Create a new issue](https://github.com/unfoldedcircle/integration-home-assistant/issues/new)
- [Reach out to us on X](https://x.com/unfoldedcircle)
- [Visit our community forum](https://unfolded.community/)
- [Chat with us in our Discord channel](http://unfolded.chat/)
- [Send us a message on our website](https://unfoldedcircle.com/contact)
