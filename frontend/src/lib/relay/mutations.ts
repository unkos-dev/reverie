import { graphql } from "react-relay";

export const mutationsTitleSnapshot = graphql`
  fragment mutationsTitleSnapshot on MetadataRevision {
    id
    workId
    manifestationId
    etag
    title
  }
`;

export const mutationsFormSnapshot = graphql`
  fragment mutationsFormSnapshot on MetadataRevision @throwOnFieldError {
    id
    workId
    manifestationId
    etag
    title
    subtitle
    description
    language
    publisher
    publicationDate
    isbn10
    isbn13
    isbnMismatch
    pages
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
    ... on EditTooLarge {
      message
    }
    ... on NotFound {
      message
    }
  }
`;

export const mutationsUndoRefusal = graphql`
  fragment mutationsUndoRefusal on UndoMetadataResult {
    ... on StaleBaseline {
      message
    }
    ... on UndoUnavailable {
      message
    }
    ... on UndoConflict {
      message
    }
    ... on FieldsLocked {
      message
      fields
    }
    ... on EditTooLarge {
      message
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
        requestId
        operations {
          operationId
          fieldName
          expiresAt
          status
        }
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

export const mutationsUndoTitleMutation = graphql`
  mutation mutationsUndoTitleMutation($input: UndoMetadataInput!) {
    undoMetadata(input: $input) {
      __typename
      ... on MetadataRestored {
        operation {
          operationId
          fieldName
          expiresAt
          status
        }
        manifestation {
          id
          metadata {
            ...mutationsTitleSnapshot @relay(mask: false)
          }
        }
      }
      ...mutationsUndoRefusal @relay(mask: false)
    }
  }
`;

export const mutationsFormQuery = graphql`
  query mutationsFormQuery($id: ID!) @throwOnFieldError {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsFormSnapshot
      }
    }
  }
`;

export const mutationsSaveFormMutation = graphql`
  mutation mutationsSaveFormMutation($input: UpdateMetadataInput!) {
    updateMetadata(input: $input) {
      __typename
      ... on MetadataSaved {
        manifestation {
          id
          metadata {
            ...mutationsFormSnapshot
          }
          ...readerDetailEditable
          ...readerVersions
        }
      }
      ...mutationsSaveRefusal @relay(mask: false)
    }
  }
`;

export const mutationsUndoFormMutation = graphql`
  mutation mutationsUndoFormMutation($input: UndoMetadataInput!) {
    undoMetadata(input: $input) {
      __typename
      ... on MetadataRestored {
        operation {
          operationId
          fieldName
          expiresAt
          status
        }
        manifestation {
          id
          metadata {
            ...mutationsFormSnapshot
          }
        }
      }
      ...mutationsUndoRefusal @relay(mask: false)
    }
  }
`;

export const mutationsResolveFormQuery = graphql`
  query mutationsResolveFormQuery($id: ID!, $requestId: UUID, $operationId: UUID)
  @throwOnFieldError {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      metadata {
        ...mutationsFormSnapshot
      }
    }
  }
`;

export const mutationsRevisionQuery = graphql`
  query mutationsRevisionQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        workId
        manifestationId
        etag
      }
    }
  }
`;

export const mutationsCreateShelfMutation = graphql`
  mutation mutationsCreateShelfMutation($input: CreateShelfInput!, $connections: [ID!]!) {
    createShelf(input: $input) {
      __typename
      ... on ShelfCreated {
        edge @appendEdge(connections: $connections) {
          cursor
          node {
            id
            name
            isSystem
            itemCount
            createdAt
            updatedAt
          }
        }
      }
      ... on ShelfNameInvalid {
        message
      }
    }
  }
`;

export const mutationsRenameShelfMutation = graphql`
  mutation mutationsRenameShelfMutation($input: RenameShelfInput!) {
    renameShelf(input: $input) {
      __typename
      ... on ShelfRenamed {
        shelf {
          id
          name
          updatedAt
        }
      }
      ... on ShelfNameInvalid {
        message
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsDeleteShelfMutation = graphql`
  mutation mutationsDeleteShelfMutation($input: DeleteShelfInput!, $connections: [ID!]!) {
    deleteShelf(input: $input) {
      __typename
      ... on ShelfDeleted {
        deletedShelfId @deleteEdge(connections: $connections)
      }
      ... on SystemShelfImmutable {
        message
      }
      ... on NotFound {
        message
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

export const mutationsAcceptMetadataVersionMutation = graphql`
  mutation mutationsAcceptMetadataVersionMutation($input: AcceptMetadataVersionInput!) {
    acceptMetadataVersion(input: $input) {
      __typename
      ... on MetadataVersionAccepted {
        manifestation {
          id
          metadata {
            ...mutationsFormSnapshot
          }
          ...readerDetailBook
          ...readerVersions
        }
      }
      ... on FieldsLocked {
        message
        fields
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          field
          message
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsRejectMetadataVersionMutation = graphql`
  mutation mutationsRejectMetadataVersionMutation($input: RejectMetadataVersionInput!) {
    rejectMetadataVersion(input: $input) {
      __typename
      ... on MetadataVersionRejected {
        manifestation {
          id
          ...readerDetailBook
          ...readerVersions
        }
      }
      ... on NotFound {
        message
      }
    }
  }
`;

export const mutationsRevertMetadataFieldMutation = graphql`
  mutation mutationsRevertMetadataFieldMutation($input: RevertMetadataFieldInput!) {
    revertMetadataField(input: $input) {
      __typename
      ... on MetadataFieldReverted {
        manifestation {
          id
          metadata {
            ...mutationsFormSnapshot
          }
          ...readerDetailBook
          ...readerVersions
        }
      }
      ... on FieldsLocked {
        message
        fields
      }
      ... on MetadataInvalid {
        message
        fieldErrors {
          field
          message
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
