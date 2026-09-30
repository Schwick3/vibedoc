# HTTPX Python reference evaluation

Evaluated September 30, 2026 with bounded project-import and type-alias resolution
after Vibedoc commit 3619950db5d8c2f15c82075585099a11cdec59c1, using Python 3.12.0.

Source: [encode/httpx](https://github.com/encode/httpx), pinned at
[b5addb64f0161ff6bfe94c124ef76f6a1fba5254](https://github.com/encode/httpx/tree/b5addb64f0161ff6bfe94c124ef76f6a1fba5254).

The repository was cloned from GitHub. No upstream dependencies were installed,
project code imported, or documentation generator executed. All tracked source
and documentation remained unchanged. Temporary mutated copies and controlled
reference documents were removed after checking.

## Original documentation

The existing docs/api.md is a developer reference, not merely a README.
It mixes generator directives such as ::: httpx.request with class headings
and handwritten member lists. This evaluation checks the Markdown source,
not the generated website.

| Check | Verified | Contradicted | Unverified | Exit |
| --- | ---: | ---: | ---: | ---: |
| Before support: unchanged API page | 0 | 0 | 0 | 1 |
| Before support: wrong native return | 0 | 0 | 0 | 1 |
| Now: unchanged API page | 4 | 0 | 14 | 0 |
| Now: wrong Response.read return | 3 | 1 | 14 | 1 |
| Explicit Response.read binding | 1 | 0 | 0 | 0 |
| Explicit binding, wrong return | 0 | 1 | 0 | 1 |
| Controlled request reference | 20 | 0 | 11 | 0 |
| Controlled request, wrong parameter | 18 | 1 | 11 | 1 |
| Before inheritance support: Client.get | 0 | 0 | 9 | 0 |
| Before type resolution: Client.get | 8 | 0 | 9 | 0 |
| Controlled Client.get reference | 11 | 0 | 6 | 0 |
| Client.get, wrong parameter | 9 | 1 | 6 | 1 |
| Client.get, wrong url type | 10 | 1 | 6 | 1 |
| Client.get, wrong follow_redirects type | 10 | 1 | 6 | 1 |
| Client.get, wrong return type | 10 | 1 | 6 | 1 |
| Client.get, unknown documented return alias | 10 | 0 | 7 | 0 |
| Controlled Client.close return | 1 | 0 | 0 | 0 |
| Client.close, wrong return | 0 | 1 | 0 | 1 |

Before support, the unchanged page produced eight VDOC-G009 errors for real
source classes the adapter did not emit, plus VDOC-G010 and one language warning.
The wrong native return produced identical diagnostics.

Class symbols and handwritten method-return recognition remove those eight
false class-name errors and the document's zero-coverage warning. Four native
return claims now verify. Fourteen remain explicitly unverified; one unrelated
qualitative-language warning remains. This is partial coverage of the page,
not verification of every class, property, signature, or generator directive.

Changing only the handwritten Response.read return from bytes to str now
produces exactly one VDOC-G006, with evidence at the method's source declaration.
The explicit-binding negative control also detects this contradiction.

## Source evidence

The adapter reads 23 Python files and emits 515 unique symbols: 428 callable
symbols and 87 classes. Previously only the 428 callables were emitted.
Symbol confidence alone does not imply that every annotation is supported.

The controlled request reference has 15 parameter names. All names, three builtin
annotations, URL | str, and the Response return now verify, totaling 20 claims.
Eleven parameter types remain unverified, including broader aliases, typing.Any,
and ssl.SSLContext. Renaming method to missing still produces VDOC-G003 and a
missing-parameter warning while unresolved types continue to abstain.

Client.get is directly declared on Client, whose same-file BaseClient ancestry
passes the existing confidence checks. Its eight parameter names, URL | str,
bool | UseClientDefault, and Response return now verify. URL and Response resolve
through explicit relative imports to selected class declarations; UseClientDefault
is an earlier local class. Normalized types retain qualified identities such as
httpx._models.Response, while displayed annotations retain their written spelling.
The other six parameter types remain incomplete because their aliases contain
forward references, unsupported typing forms, or Any.

Renaming url to missing produces VDOC-G003 with the exact Client.get declaration
as evidence, plus VDOC-G004 for the missing source parameter. Replacing either
newly supported parameter type with int produces VDOC-G005 at that parameter's
source declaration. Replacing Response with str produces VDOC-G006 at Client.get.
An unknown documented alias remains unverified, rather than being classified as
a different type solely because its spelling differs.

Client.close still verifies None and rejects str with the exact source location.
Native API coverage remains four verified and fourteen unverified claims; its
wrong Response.read return is still detected. No generator directives or new
Markdown formats were added. The python-dotenv evaluation separately improves
load_dotenv from ten to eleven verified claims through Optional[str].

Native rows referring to absent or incomplete members remain unverified.
For example, Response.next and Response.anext have no matching direct method
facts, while Headers.copy is under an incompletely modeled class.

Controlled references are generated by the test from adapter signatures and
explicit source paths. Positive counts demonstrate the supported path, not
independent agreement with upstream prose. Negative controls deliberately
change names or types. The Response.read test additionally asserts that its
source return annotation is exactly bytes.

## Boundaries and next work

Handwritten rows check return types only. Their parameter text is not verified.
Constructors, properties, and generator directives remain outside this subset.
Class headings alone do not count as verified claims.

Supported inheritance is limited to unambiguous, earlier module-level bases
within one source file, ending at no base or unshadowed builtin object.
Unknown ancestry, class decorators, metaclasses, ancestor hooks, rebinding,
and method-level uncertainty continue to abstain. Inherited-only methods are
not synthesized.

Type resolution follows only explicit relative imports to directly declared classes
or assignment aliases in selected files. Conditional imports, re-export chains,
absolute project imports, quoted forward references, unsupported typing forms,
and unresolved document-side aliases remain outside the subset. Cross-file
inheritance and additional documentation formats remain out of scope.

## Reproduce

From the Vibedoc checkout:

    cargo build --workspace --locked
    npm ci
    git clone https://github.com/encode/httpx.git /tmp/vibedoc-httpx
    git -C /tmp/vibedoc-httpx checkout --detach b5addb64f0161ff6bfe94c124ef76f6a1fba5254
    python3 scripts/test-httpx-project.py /tmp/vibedoc-httpx /tmp/httpx-results.json

On this machine, the default Xcode selection required license acceptance.
The build succeeded using the separately installed Command Line Tools via
DEVELOPER_DIR=/Library/Developer/CommandLineTools, without modifying system
settings.

The script checks the pinned revision, unique source IDs, exact claim counts,
report schemas, negative controls, source evidence, and preservation of the
original API page. CI includes this pinned evaluation. The adapter integration
suite separately covers duplicate class names, explicit-binding overrides,
same-named members on other classes, incomplete classes, and fenced examples.

[Machine-readable results](httpx-results.json) include the original Markdown
SHA-256, selected source signatures, and diagnostics for every check.
