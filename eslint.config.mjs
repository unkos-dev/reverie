import graphqlPlugin from "@graphql-eslint/eslint-plugin";

const [, naming] =
  graphqlPlugin.configs["flat/schema-recommended"].rules["@graphql-eslint/naming-convention"];

export default [
  {
    files: ["backend/schema.graphql"],
    languageOptions: {
      parser: graphqlPlugin.parser,
      parserOptions: { graphQLConfig: { schema: "backend/schema.graphql" } },
    },
    plugins: { "@graphql-eslint": graphqlPlugin },
    rules: {
      ...graphqlPlugin.configs["flat/schema-relay"].rules,
      "@graphql-eslint/relay-arguments": ["error", { includeBoth: false }],
      "@graphql-eslint/relay-edge-types": [
        "error",
        { shouldImplementNode: false, listTypeCanWrapOnlyEdgeType: false },
      ],
      // fetch__* roots and the field_name argument are Relay's @fetchable contract.
      "@graphql-eslint/naming-convention": [
        "error",
        {
          ...naming,
          FieldDefinition: { style: "camelCase", ignorePattern: "^fetch__" },
          InputValueDefinition: { style: "camelCase", ignorePattern: "^field_name$" },
          Argument: { style: "camelCase", ignorePattern: "^field_name$" },
        },
      ],
    },
  },
];
