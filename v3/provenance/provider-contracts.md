# Provider source verification (2026-10-25)

Production URLs and parsing preserve the native adapter from Tezbar
fe71ca87f21ac4ff31505010c228f8abda0f2a89 and the original TypeScript sources in
this repository. The public upstream comparison is pinned separately to
raycast/extensions 97ee2594a03a94fa439c26864d85ad9a96dc9fc9, without claiming
that commit is the installed asset revision.

[GIPHY API documentation](https://developers.giphy.com/docs/api/) describes
authenticated direct endpoints and offset pagination. The extension preserves
its existing Raycast proxy contract; direct API keys are not embedded or invented.
The documentation search result was available; opening that page timed out.

[Upstream Klipy source](https://raw.githubusercontent.com/raycast/extensions/97ee2594a03a94fa439c26864d85ad9a96dc9fc9/extensions/gif-search/src/models/klipy.ts)
confirms the Raycast proxy, locale/media filter/limit, pos cursor, results/next,
media_formats and ID lookups. [Direct Klipy docs](https://docs.klipy.com/) expose
API-key provisioning; they do not establish permission or continued availability
of the Raycast proxy for a separate application. No live API probe was made.

[Upstream Finer source](https://raw.githubusercontent.com/raycast/extensions/97ee2594a03a94fa439c26864d85ad9a96dc9fc9/extensions/gif-search/src/models/finergifs.ts)
confirms search size/start/q.parser/sort, fileid lookups in batches of ten,
results/fields/fileid, media URL mapping, and the absence of trending results.
No independent Finer service availability guarantee is inferred from source.

The local HTTP fixtures assert these query/result contracts, paging, sanitized
errors, media, and cancellation using the real executable. They do not prove live
provider availability or desktop clipboard behavior.
