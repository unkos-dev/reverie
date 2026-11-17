---
severity: low
surfaces: [developer]
adopted: 2026-11-18
adopted-because: Juniper 0.18.0 refuses variable input objects that omit a non-null field with a default value
lift-when-class: dep-unblocks
lift-when: a released Juniper version applies input-field default values in variable validation and the GraphQL backend tests pass without the default-filling helper
---

# Juniper variable input defaults

The GraphQL specification lets a variable's input object omit a non-null field that has a default value. Juniper 0.18.0
applies those defaults to literal arguments, but its variable validation (`unify_input_object` in
`validation/input_value.rs`) reports the absent field as `found null`. `BooksFilter` declares its list fields as
`[ID!]! = []` or `[String!]! = []`, and the Library page sends partial filter variables, so every filtered Library
request would fail.

`with_input_defaults` in `backend/src/graphql/mod.rs` walks the selected operation's variable definitions and adds each
absent field that has a schema default, including fields inside lists and nested input objects, before Juniper validates
the variables. `graphql_cursor_contract` sends partial `BooksFilter` variables and fails without it.

When upgrading to a Juniper release whose variable validation applies input-field defaults, delete `with_input_defaults`
and `fill_input_defaults`, pass the request variables straight to execution, and remove this entry and its README link
in the same change. The backend GraphQL tests must pass without the helper.
