<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="AuthenticationService.login" -->
# `AuthenticationService.login`

`AuthenticationService.login` validates an email address.

## Parameters

- `email` (`string`): The email address.
- `remember` (`boolean`): Whether the session persists.

## Returns

`Promise<User>`

## Errors

- `InvalidCredentialsError`: The email address is not valid.

