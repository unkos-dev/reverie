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
  fragment mutationsFormSnapshot on MetadataRevision {
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
    contentRating
  }
`;

export const mutationsReadingSnapshot = graphql`
  fragment mutationsReadingSnapshot on ReadingState {
    etag
    status
    rating
    notes
    progressPct
    startedAt
    finishedAt
    lastReadAt
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

export const mutationsResolveTitleQuery = graphql`
  query mutationsResolveTitleQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsTitleSnapshot @relay(mask: false)
      }
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
            ...mutationsFormSnapshot @relay(mask: false)
          }
        }
      }
      ...mutationsUndoRefusal @relay(mask: false)
    }
  }
`;

export const mutationsResolveFormQuery = graphql`
  query mutationsResolveFormQuery($id: ID!, $requestId: String, $operationId: String)
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

export const mutationsReadingQuery = graphql`
  query mutationsReadingQuery($id: ID!) {
    manifestation(id: $id) {
      id
      readingState {
        ...mutationsReadingSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsReadingMutation = graphql`
  mutation mutationsReadingMutation($id: ID!, $baseline: String, $patch: ReadingPatch!) {
    updateReadingState(id: $id, baseline: $baseline, patch: $patch) {
      ...mutationsReadingSnapshot @relay(mask: false)
    }
  }
`;

export const mutationsAuthorPageQuery = graphql`
  query mutationsAuthorPageQuery($id: ID!, $first: Int!, $after: String) {
    manifestation(id: $id) {
      id
      metadata {
        workId
        manifestationId
        etag
        authors(first: $first, after: $after) {
          edges {
            cursor
            node {
              name
              author {
                id
              }
            }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
      }
    }
  }
`;

export const mutationsSaveAuthorsMutation = graphql`
  mutation mutationsSaveAuthorsMutation($input: UpdateMetadataInput!) {
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
            workId
            manifestationId
            etag
            authors(first: 20) {
              edges {
                cursor
                node {
                  name
                  author {
                    id
                  }
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        }
      }
      ...mutationsSaveRefusal @relay(mask: false)
    }
  }
`;

export const mutationsUndoAuthorsMutation = graphql`
  mutation mutationsUndoAuthorsMutation($input: UndoMetadataInput!) {
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
            workId
            manifestationId
            etag
            authors(first: 20) {
              edges {
                cursor
                node {
                  name
                  author {
                    id
                  }
                }
              }
              pageInfo {
                hasNextPage
                hasPreviousPage
                startCursor
                endCursor
              }
            }
          }
        }
      }
      ...mutationsUndoRefusal @relay(mask: false)
    }
  }
`;

export const mutationsResolveAuthorsQuery = graphql`
  query mutationsResolveAuthorsQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        workId
        manifestationId
        etag
        authors(first: 20) {
          edges {
            cursor
            node {
              name
              author {
                id
              }
            }
          }
          pageInfo {
            hasNextPage
            hasPreviousPage
            startCursor
            endCursor
          }
        }
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
  mutation mutationsUpdateLibraryPreferencesMutation($patch: LibraryPreferencesPatch!) {
    updateLibraryPreferences(patch: $patch) {
      ...readerLibraryPreferences
    }
  }
`;

export const mutationsRefreshMetadataMutation = graphql`
  mutation mutationsRefreshMetadataMutation($id: ID!) {
    refreshMetadata(id: $id) {
      manifestationId
    }
  }
`;

export const mutationsCellsSnapshot = graphql`
  fragment mutationsCellsSnapshot on MetadataRevision {
    id
    workId
    manifestationId
    etag
    title
    subtitle
    isbn13
    pages
  }
`;

export const mutationsCellsQuery = graphql`
  query mutationsCellsQuery($id: ID!) {
    manifestation(id: $id) {
      id
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;

export const mutationsSaveCellsMutation = graphql`
  mutation mutationsSaveCellsMutation($input: UpdateMetadataInput!) {
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
            ...mutationsCellsSnapshot @relay(mask: false)
          }
        }
      }
      ...mutationsSaveRefusal @relay(mask: false)
    }
  }
`;

export const mutationsUndoCellsMutation = graphql`
  mutation mutationsUndoCellsMutation($input: UndoMetadataInput!) {
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
            ...mutationsCellsSnapshot @relay(mask: false)
          }
        }
      }
      ...mutationsUndoRefusal @relay(mask: false)
    }
  }
`;

export const mutationsResolveCellsQuery = graphql`
  query mutationsResolveCellsQuery($id: ID!, $requestId: String, $operationId: String) {
    metadataOperation(id: $id, requestId: $requestId, operationId: $operationId) {
      outcome
      operations {
        operationId
        fieldName
        expiresAt
        status
      }
      metadata {
        ...mutationsCellsSnapshot @relay(mask: false)
      }
    }
  }
`;
