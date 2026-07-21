# Product

## Register

brand

## Platform

web

## Users

The documentation site serves two overlapping audiences, both arriving from outside the project.

The **prospective self-hoster** is the primary audience. They arrive from GitHub, a link, or a search, having never seen Reverie before. Their context is evaluative and short: a few minutes deciding whether this is worth their attention, on a laptop, probably with several other candidate projects open in adjacent tabs. Their job is to answer "what is this, is it any good, and can I run it?" They are technically capable, they self-host by choice, and they have been disappointed by ugly self-hosted software before.

The **operator already running Reverie** is the secondary audience. They return with a specific question, usually about configuration, the API, or a behaviour they did not expect. Their context is task-driven: they arrive via search or the sidebar, want one fact, and leave. Nothing on the site should slow that path down.

Contributors and the design workstream are not an audience the site is shaped for; their reference material (the design-system pages) lives in the repository under `docs/design/`, not on the published site.

## Product Purpose

The documentation site is the only public surface of Reverie that is not a code repository. It has to introduce the product to people who have never seen it, and then answer questions for the people who chose to run it.

Reverie is pre-alpha with no tagged release; until the first release ships, the only published image is the arm64 staging build of main. The site does not hide that, and the primary conversion is therefore attention rather than installation: someone who cannot run it today should still leave having decided to follow it.

Success looks like:

- A visitor who arrives cold gets an accurate picture of what Reverie is and how it is built, and follows the repository rather than closing the tab.
- An operator with a configuration question finds the answer without asking, so the issue tracker carries defects rather than support.
- Generated reference never drifts from the code, and hand-written pages never contradict the app.

## Positioning

Reverie is a self-hosted ebook library manager that treats a personal collection as a permanent record rather than a folder of files, and looks like it. The documentation site's job is to make that visible before anyone installs anything.

## Conversion & proof

- Primary CTA: star or watch the repository on GitHub. No release is tagged yet, so banking interest from people who cannot run it yet is worth more today than pushing a container command at them.
- Secondary CTAs: the getting-started introduction for people ready to run it, and the generated API reference, which is real, complete, and demonstrates the engineering to a visitor who is curious but not ready to commit.
- The line a visitor remembers after ten seconds is the tagline: Your library, catalogued. It is the only tagline; no second line is paired with it.
- Belief ladder, in order: first, the site is finished and deliberate, carried by the splash page, the palette, and the writing. Second, the engineering underneath is real, carried by the generated OpenAPI reference, the ADRs, the CI gates, and an open repository. Third, it is not ready for me today but I want to know when it is, carried by stating the pre-alpha status plainly rather than burying it.
- Proof on hand: the engineering itself. There are no users, testimonials, press mentions, or partner logos, and none should be implied. The credentials available are the open repository, the code-first generated OpenAPI reference, the recorded architecture decisions, the CI gates, and the test suite. For an audience deciding whether to trust a container with their library, visible rigour is the credential.

## Brand Personality

The site inherits Reverie's personality rather than inventing one: editorial, weighted, permanent, with a cinematic-boutique register on identity surfaces. The voice is restrained, deliberate, and British-spelled. No breezy SaaS copy, no exclamation marks, no growth-hook enthusiasm.

Documentation adds one constraint of its own. Prose on reference pages is plain and answers the question asked; personality lives in the splash page, not in the configuration tables. A page whose job is to tell an operator what a setting does should read as though written by someone who respects the reader's time.

The current identity budget is deliberate and partial. The splash page carries the full brand: palette, mark, typography, and the persuasion. The reference and guide pages inherit palette and mark only and otherwise keep Starlight's proven reading layout and type scale. Full inheritance across every page is a possible future, not a present commitment.

## Anti-references

- **Startup-launch landing page.** Gradient hero, feature triptych of icon-cards, a big metric row, a "trusted by" logo wall, a waitlist form. Reverie has no users, so none of that grammar can be used truthfully.
- **Corporate enterprise documentation portal.** Dense breadcrumbs, a product-suite switcher, version tabs, and the tone of a support contract. Reverie is one person's project and should read like it.
- **Overdesigned documentation that fights reading.** Identity applied so heavily that finding a configuration key becomes work: decorative page transitions, low-contrast body text chosen for elegance, custom code blocks worse than the default. The reference pages are instruments.
- **The untouched Astro starter.** The state the site shipped in before the splash work: the default sparkle favicon, the Houston mascot, system-ui, and Starlight's purple accent. It read as unfinished, which contradicted the first rung of the belief ladder; it is kept here so the site never regresses toward it.

Reverie's own anti-references carry over in full, because the site shows the product: SaaS-cream workflow chrome, generic ebook UI, the cozy reading nook, and severity-coloured dashboards.

## Design Principles

1. **The splash persuades; the pages inform.** One surface does identity work and one does reference work, and neither should be asked to do the other's job. Branding effort concentrates where a stranger forms an opinion.

2. **Honest about pre-alpha.** The constraints are stated plainly, early, and without apology. A visitor who discovers the staging image's platform limits after following an install path has been mistreated; one who reads the status on the front page and follows anyway has been converted properly.

3. **Rigour is the proof, so show it rather than claim it.** With no users to cite, credibility comes from the generated reference, the recorded decisions, and the open repository being visible and reachable. No testimonial-shaped placeholders, no invented social proof.

4. **Reading speed is a feature.** An operator looking for one fact should reach it without passing through decoration. Any identity treatment that costs scannability on a reference page is the wrong trade.

5. **Documentation tracks the code, not the intention.** Generated pages are regenerated, never hand-patched, and hand-written pages are corrected when they disagree with the application. A page that contradicts the product is worse than a page that does not exist.

## Accessibility & Inclusion

No formal conformance target is adopted for the documentation site. Starlight ships accessible defaults, maintained upstream by people who take it seriously, and the working rule is to preserve them rather than to certify against a standard.

Practically, this means custom theming must not regress what Starlight provides: keep contrast at or above the stock theme's, leave focus visibility intact, do not remove or weaken the skip link or landmark structure, and honour `prefers-reduced-motion` on anything added to the splash page. A branded palette that would lower contrast below the default theme gives way.

The application holds a stricter target of WCAG 2.2 Level AA as a design invariant. That target belongs to the application and is not inherited here.
