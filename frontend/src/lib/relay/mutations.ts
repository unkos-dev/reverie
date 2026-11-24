import { graphql } from "react-relay";

export const mutationsTitleSnapshot = graphql`
  fragment mutationsTitleSnapshot on MetadataRevision {
    id
    etag
    title
  }
`;

export const mutationsSaveRefusal = graphql`
  fragment mutationsSaveRefusal on UpdateMetadataResult {
    ... on StaleBaseline {
      message
    }
    ... on MetadataInvalid {
      message
      fieldErrors {
        field
        message
      }
    }
    ... on FieldsLocked {
      message
      fields
    }
    ... on NotFound {
      message
    }
  }
`;

export const mutationsTitleQuery = graphql`
  query mutationsTitleQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsSaveTitleMutation = graphql`
  mutation mutationsSaveTitleMutation($input: UpdateMetadataInput!) {
    updateMetadata(input: $input) {
      __typename
      ... on MetadataSaved {
        manifestation {
          id
          metadata {
            ...mutationsTitleSnapshot @relay(mask: false)
          }
        }
      }
      ...mutationsSaveRefusal @relay(mask: false)
    }
  }
`;

export const mutationsRevisionQuery = graphql`
  query mutationsRevisionQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        etag
      }
    }
  }
`;

export const mutationsAddShelfItemMutation = graphql`
  mutation mutationsAddShelfItemMutation($input: AddShelfItemInput!, $connections: [ID!]!) {
    addShelfItem(input: $input) {
      __typename
      ... on ShelfItemAdded {
        shelf {
          id
          itemCount
          updatedAt
        }
        edge @appendEdge(connections: $connections) {
          cursor
          node {
            id
          }
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsLockMetadataFieldMutation = graphql`
  mutation mutationsLockMetadataFieldMutation($input: LockMetadataFieldInput!) {
    lockMetadataField(input: $input) {
      __typename
      ... on MetadataFieldLockAdded {
        manifestation {
          id
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsUnlockMetadataFieldMutation = graphql`
  mutation mutationsUnlockMetadataFieldMutation($input: UnlockMetadataFieldInput!) {
    unlockMetadataField(input: $input) {
      __typename
      ... on MetadataFieldLockRemoved {
        manifestation {
          id
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsUpdateLibraryPreferencesMutation = graphql`
  mutation mutationsUpdateLibraryPreferencesMutation($input: UpdateLibraryPreferencesInput!) {
    updateLibraryPreferences(input: $input) {
      __typename
      ... on LibraryPreferencesUpdated {
        viewer {
          id
          libraryPreferences {
            ...useLibraryPreferences_preferences
          }
        }
      }
      ... on LibraryPreferencesInvalid {
        message
      }
    }
  }
`;
